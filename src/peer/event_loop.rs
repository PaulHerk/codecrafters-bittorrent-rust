use std::mem;

use futures_util::StreamExt;

use crate::{
    messages::{
        PeerMessage,
        payloads::{HavePayload, NoPayload},
    },
    peer::{Msg, Peer, error::PeerError},
    peer_manager::{ReqMessage, ResMessage},
};

impl Peer {
    pub async fn run(mut self) -> Result<(), PeerError> {
        // TODO: do choking
        *self.state.0.am_choking.lock().unwrap() = false;

        let mut blocks_left_for_queue = 0_u32;

        let mut receiver_stream = mem::take(&mut self.receiver_stream)
            .expect("The receiver stream is initialized after creation of the peer.");
        // ask peer_manager what we have
        self.send_peer_manager(ReqMessage::WhatDoWeHave).await?;
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
                            self.send_peer(PeerMessage::Have(have_payload)).await?;
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
                                self.send_peer(PeerMessage::Piece(payload)).await?;
                            }
                            // TODO: else?
                        }
                        ResMessage::WeHave(bitfield) => {
                            // later TODO: implement lazy bitfield?
                            if !bitfield.is_nothing() {
                                self.send_peer(PeerMessage::Bitfield(bitfield)).await?;
                            }
                        }
                    },
                    Msg::Data(message) => {
                        match message {
                            PeerMessage::Choke(_no_payload) => {
                                *self.state.0.peer_choking.lock().unwrap() = true;
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
                                self.send_peer_manager(ReqMessage::NeedBlockQueue).await?;
                            }
                            PeerMessage::Request(request_piece_payload) => {
                                self.send_peer_manager(ReqMessage::NeedBlock(
                                    request_piece_payload,
                                ))
                                .await?;
                            }
                            PeerMessage::Piece(response_piece_payload) => {
                                eprintln!(
                                    "got {}th block in piece {}",
                                    response_piece_payload.begin, response_piece_payload.index
                                );
                                self.send_peer_manager(ReqMessage::GotBlock(
                                    response_piece_payload,
                                ))
                                .await?;
                                blocks_left_for_queue -= 1;
                            }
                            PeerMessage::Cancel(_request_piece_payload) => todo!(), // only for end-game, won't probably use it
                            PeerMessage::KeepAlive(_no_payload) => {
                                eprintln!("he sent a keep alive")
                            }
                        }
                    }
                    Msg::Timeout => {
                        self.send_peer(PeerMessage::KeepAlive(NoPayload)).await?;
                    }
                }

                // request next blocks
                if blocks_left_for_queue == 0
                    && *self.state.0.am_interested.lock().unwrap()
                    && !*self.state.0.peer_choking.lock().unwrap()
                {
                    // I don't like doing this, however self.send_peer() takes a mut ref to self and thus I can't iterate over self.req_queue at the same time
                    let queue_iter: Vec<_> = self.req_queue.iter().cloned().collect();
                    for req in queue_iter {
                        let req_msg = PeerMessage::Request(req);
                        self.send_peer(req_msg).await?;
                    }
                    blocks_left_for_queue = self.req_queue.len() as u32;
                    self.req_queue.clear();
                    self.send_peer_manager(ReqMessage::NeedBlockQueue).await?;
                }
            } else {
                eprintln!("peer disconnected");
                break Err(PeerError::PeerDisconnected);
            }
        }?;
        self.receiver_stream = Some(receiver_stream);
        Ok(())
    }
}
