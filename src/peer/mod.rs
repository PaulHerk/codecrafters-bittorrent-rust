use futures_util::{self, SinkExt, StreamExt};
use tokio::sync::mpsc;

use anyhow::Context;

use crate::conn::{BoxedMsgStream, Peer};
use crate::messages::payloads::*;
use crate::{messages::*, *};

pub mod conn;
pub mod handshake;
pub mod states;

/// this enum is used to select between different stream-types a peer can receive
#[derive(Debug, PartialEq)]
pub enum Msg {
    /// this will be sent to other peers in order to announce that it has the piece
    ManagerMsg(ResMessage),
    Data(PeerMessage),
    Timeout,
}

impl Peer {
    pub async fn event_loop(&mut self, mut receiver_stream: BoxedMsgStream) -> anyhow::Result<()> {
        // assert_eq!(self.state, PeerState::DataTransfer);
        // TODO: do choking
        *self.state.0.am_choking.lock().unwrap() = false;

        let mut blocks_left_for_queue = 0_u32;

        // ask req_manager what we have
        self.send_req_manager(ReqMessage::WhatDoWeHave)
            .await
            .context("asking for our bitfield")?;
        loop {
            if let Some(message) = receiver_stream.next().await {
                if let Msg::Data(PeerMessage::Piece(_)) = message {
                } else {
                    // dbg!(&message);
                }
                match message {
                    Msg::ManagerMsg(peer_msg) => match peer_msg {
                        ResMessage::FinishedFile => {
                            // TODO: set flags
                            if self.sever_conn().await {
                                break Ok(());
                            }
                        }
                        ResMessage::FinishedPiece(piece_index) => {
                            // later TODO: implement have suppression
                            let have_payload = HavePayload { piece_index };
                            self.peer_writer
                                .send(PeerMessage::Have(have_payload))
                                .await
                                .context("send have")?;
                        }
                        ResMessage::NewBlockQueue(request_piece_payloads) => {
                            self.req_queue.extend_from_slice(&request_piece_payloads);
                            if self.req_queue.is_empty() {
                                self.set_interested(false).await?;
                            } else {
                                self.set_interested(true).await?;
                            };
                        }
                        ResMessage::Block(response_piece_payload) => {
                            if let Some(payload) = response_piece_payload {
                                self.peer_writer
                                    .send(PeerMessage::Piece(payload))
                                    .await
                                    .context("Sending block")?;
                            }
                            // TODO: else?
                        }
                        ResMessage::WeHave(bitfield) => {
                            // later TODO: implement lazy bitfield?
                            if !bitfield.is_nothing() {
                                self.peer_writer
                                    .send(PeerMessage::Bitfield(bitfield))
                                    .await
                                    .context("sending our bitfield")?;
                            }
                        }
                    },
                    Msg::Data(message) => {
                        match message {
                            PeerMessage::Choke(_no_payload) => {
                                *self.state.0.peer_choking.lock().unwrap() = true;
                                // return Err(anyhow::anyhow!("peer is choked"));
                            }
                            PeerMessage::Unchoke(_no_payload) => {
                                *self.state.0.peer_choking.lock().unwrap() = false
                            }
                            PeerMessage::Interested(_no_payload) => {
                                *self.state.0.peer_interested.lock().unwrap() = true
                            }
                            PeerMessage::NotInterested(_no_payload) => {
                                *self.state.0.peer_interested.lock().unwrap() = false
                            }
                            PeerMessage::Have(have_payload) => {
                                self.state.0.has.lock().unwrap()
                                    [have_payload.piece_index as usize] = true;
                            }
                            PeerMessage::Bitfield(bitfield_payload) => {
                                *self.state.0.has.lock().unwrap() =
                                    bitfield_payload.pieces_available;
                                // TODO: we might want to do it more efficient than requesting the whole queue
                                // this currently starts the whole loop by setting the interested flag eventually
                                // self.req_next_block(&mut peer_writer).await?;
                                self.send_req_manager(ReqMessage::NeedBlockQueue).await?;
                            }
                            PeerMessage::Request(request_piece_payload) => {
                                self.send_req_manager(ReqMessage::NeedBlock(request_piece_payload))
                                    .await
                                    .context("requesting ReqManager for block")?;
                            }
                            PeerMessage::Piece(response_piece_payload) => {
                                eprintln!(
                                    "got {}th block in piece {}",
                                    response_piece_payload.begin, response_piece_payload.index
                                );
                                self.send_req_manager(ReqMessage::GotBlock(response_piece_payload))
                                    .await
                                    .context("sending that we got block to ReqManager")?;
                                blocks_left_for_queue -= 1;
                            }
                            PeerMessage::Cancel(_request_piece_payload) => todo!(), // only for end-game, won't probably use it
                            PeerMessage::KeepAlive(_no_payload) => {
                                eprintln!("he sent a keep alive")
                            }
                        }
                    }
                    Msg::Timeout => {
                        self.peer_writer
                            .send(PeerMessage::KeepAlive(NoPayload))
                            .await?
                    }
                }

                // request next blocks
                if blocks_left_for_queue == 0
                    && *self.state.0.am_interested.lock().unwrap()
                    && !*self.state.0.peer_choking.lock().unwrap()
                {
                    for req in self.req_queue.iter() {
                        let req_msg = PeerMessage::Request(req.clone());
                        self.peer_writer.send(req_msg).await?;
                    }
                    blocks_left_for_queue = self.req_queue.len() as u32;
                    self.req_queue.clear();
                    self.send_req_manager(ReqMessage::NeedBlockQueue).await?;
                }
            } else {
                eprintln!("peer disconnected");
                break Err(anyhow::anyhow!("peer disconnected"));
            }
        }
    }

    async fn send_req_manager(
        &self,
        msg: ReqMessage,
    ) -> Result<(), mpsc::error::SendError<ReqMsgFromPeer>> {
        self.req_manager_tx
            .send(ReqMsgFromPeer {
                peer_id: self.state.0.peer_id,
                msg,
            })
            .await
    }

    /// this sets our interested flag and sends the message to the peer
    async fn set_interested(&mut self, interested: bool) -> anyhow::Result<()> {
        {
            let am_interested = &mut *self.state.0.am_interested.lock().unwrap();
            if interested != *am_interested {
                *am_interested = interested;
            } else {
                // question: is am_interested dropped if I'd just do if (equal) return Ok(())?
                return Ok(());
            }
        }

        let msg = if interested {
            PeerMessage::Interested(NoPayload)
        } else {
            PeerMessage::NotInterested(NoPayload)
        };
        self.peer_writer
            .send(msg)
            .await
            .context("sending interested")?;
        Ok(())
    }

    async fn sever_conn(&mut self) -> bool {
        if *self.state.0.peer_interested.lock().unwrap()
            || *self.state.0.am_interested.lock().unwrap()
        {
            return false;
        }

        if self.peer_writer.close().await.is_err() {
            false
        } else {
            true
        }
    }
}
