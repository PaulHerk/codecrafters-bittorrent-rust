use futures_util::stream::{SplitSink, unfold};
use futures_util::{self, SinkExt, StreamExt};
use std::net::SocketAddrV4;
use tokio::sync::mpsc;
use tokio_util::codec::Framed;

use anyhow::Context;

use crate::messages::payloads::*;
use crate::states::PeerState;
use crate::{messages::*, *};

pub mod handshake;
// pub mod peer_data;
pub mod states;

type PeerWriter = SplitSink<Framed<TcpStream, MessageFramer>, PeerMessage>;

#[derive(Debug)]
pub struct Peer {
    pub peer_id: [u8; 20],
    pub addr: SocketAddrV4,
    pub state: PeerState,
    pub am_choking: bool,
    pub am_interested: bool,
    pub peer_choking: bool,
    pub peer_interested: bool,
    /// the bitfield of the other peer
    pub has: Vec<bool>,
    pub req_queue: Vec<RequestPiecePayload>,
    req_manager_tx: mpsc::Sender<ReqMsgFromPeer>,
}

/// this enum is used to select between different stream-types a peer can receive
#[derive(Debug, PartialEq)]
enum Msg {
    /// this will be sent to other peers in order to announce that it has the piece
    ManagerMsg(ResMessage),
    Data(PeerMessage),
    Timeout,
}

impl Peer {
    pub fn new(
        peer_id: [u8; 20],
        addr: SocketAddrV4,
        req_manager_tx: mpsc::Sender<ReqMsgFromPeer>,
    ) -> Self {
        Self {
            peer_id,
            addr,
            state: PeerState::default(),
            am_choking: true,
            am_interested: false,
            peer_choking: true,
            peer_interested: false,
            has: Vec::new(),
            req_queue: Vec::new(),
            req_manager_tx,
        }
    }
    /// info_hash: the info hash of the torrent
    /// peer_id: a unique identifier for your client
    pub async fn shake_hands_get_framed(
        &mut self,
        info_hash: &[u8; 20],
    ) -> anyhow::Result<MsgFrameType> {
        let mut tcp = tokio::net::TcpStream::connect(self.addr)
            .await
            .context("connect to peer")?;
        self.state = PeerState::Connected;

        let handshake_to_send = Handshake::new(*info_hash, self.peer_id);
        let handshake_recv = handshake_to_send
            .shake_hands(&mut tcp)
            .await
            .context("peer should send handshake")?;

        assert_eq!(handshake_recv.info_hash, *info_hash); // TODO: sever connection
        // assert_eq!(handshake_recv.peer_id, self.peer_id);

        self.state = PeerState::DataTransfer;

        let framed = Framed::new(tcp, MessageFramer);

        println!("peer {} connected", self.addr);
        Ok(framed)
    }

    pub async fn event_loop(&mut self, framed: MsgFrameType) -> anyhow::Result<()> {
        assert_eq!(self.state, PeerState::DataTransfer);
        let (tx, req_manager_rx) = mpsc::channel(8);
        let peer_conn = PeerConn {
            sender: tx,
            has: BitfieldPayload::new(),
        };
        self.send_req_manager(ReqMessage::NewConnection(peer_conn))
            .await
            .context("sending new peer data to ReqManager.")?;

        // TODO: do choking
        self.am_choking = false;

        let (mut peer_writer, framed_rx) = framed.split();

        let peer_msg_stream = unfold(framed_rx, |mut framed| async move {
            let Ok(Ok(message)) = framed.next().await.context("read message") else {
                return None;
            };
            Some((Msg::Data(message), framed))
        });
        tokio::pin!(peer_msg_stream);

        // this is the stream sent by other connections to peers to send have messages
        let manager_stream = unfold(req_manager_rx, |mut rx| async move {
            let Some(msg) = rx.recv().await else {
                return None;
            };
            Some((Msg::ManagerMsg(msg), rx))
        });
        tokio::pin!(manager_stream);

        // TODO: I shouldn't send a timeout every 120s
        // rather I should send it if I haven't received a message for 120s
        let duration = std::time::Duration::from_secs(120);
        let timeout = tokio::time::interval_at(tokio::time::Instant::now() + duration, duration);
        let timeout_stream = unfold(timeout, |mut timeout| async move {
            timeout.tick().await;
            Some((Msg::Timeout, timeout))
        });
        tokio::pin!(timeout_stream);

        let mut stream =
            futures_util::stream_select!(peer_msg_stream, manager_stream, timeout_stream);
        let mut waiting_for_piece = false;

        // ask req_manager what we have
        self.send_req_manager(ReqMessage::WhatDoWeHave)
            .await
            .context("asking for our bitfield")?;
        loop {
            if let Some(message) = stream.next().await {
                if let Msg::Data(PeerMessage::Piece(_)) = message {
                } else {
                    // dbg!(&message);
                }
                match message {
                    Msg::ManagerMsg(peer_msg) => match peer_msg {
                        ResMessage::FinishedFile => {
                            // TODO: set flags
                            if self.sever_conn(&mut peer_writer).await {
                                break Ok(());
                            }
                        }
                        ResMessage::FinishedPiece(piece_index) => {
                            // later TODO: implement have suppression
                            let have_payload = HavePayload { piece_index };
                            peer_writer
                                .send(PeerMessage::Have(have_payload))
                                .await
                                .context("send have")?;
                        }
                        ResMessage::NewBlockQueue(request_piece_payloads) => {
                            self.req_queue.extend_from_slice(&request_piece_payloads);
                            if self.req_queue.is_empty() {
                                self.set_interested(&mut peer_writer, false).await?;
                            } else {
                                self.set_interested(&mut peer_writer, true).await?;
                            };
                        }
                        ResMessage::Block(response_piece_payload) => {
                            if let Some(payload) = response_piece_payload {
                                peer_writer
                                    .send(PeerMessage::Piece(payload))
                                    .await
                                    .context("Sending block")?;
                            }
                            // TODO: else?
                        }
                        ResMessage::WeHave(bitfield) => {
                            // later TODO: implement lazy bitfield?
                            if !bitfield.is_nothing() {
                                peer_writer
                                    .send(PeerMessage::Bitfield(bitfield))
                                    .await
                                    .context("sending our bitfield")?;
                            }
                        }
                    },
                    Msg::Data(message) => {
                        match message {
                            PeerMessage::Choke(_no_payload) => {
                                self.peer_choking = true;
                                // return Err(anyhow::anyhow!("peer is choked"));
                            }
                            PeerMessage::Unchoke(_no_payload) => self.peer_choking = false,
                            PeerMessage::Interested(_no_payload) => self.peer_interested = true,
                            PeerMessage::NotInterested(_no_payload) => self.peer_interested = false,
                            PeerMessage::Have(have_payload) => {
                                self.has[have_payload.piece_index as usize] = true;

                                self.send_req_manager(ReqMessage::PeerHas(have_payload))
                                    .await
                                    .context("sending have message to ReqManager")?;
                            }
                            PeerMessage::Bitfield(bitfield_payload) => {
                                self.has = bitfield_payload.pieces_available.clone();
                                self.send_req_manager(ReqMessage::PeerBitfield(bitfield_payload))
                                    .await
                                    .context("sending bitfield to ReqManager")?;
                                // TODO: we might want to do it more efficient than requesting the whole queue
                                // this currently starts the whole loop by setting the interested flag eventually
                                self.req_next_block(&mut peer_writer).await?;
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
                                waiting_for_piece = false;
                            }
                            PeerMessage::Cancel(_request_piece_payload) => todo!(), // only for extension, won't probably use it
                            PeerMessage::KeepAlive(_no_payload) => {
                                eprintln!("he sent a keep alive")
                            }
                        }
                    }
                    Msg::Timeout => peer_writer.send(PeerMessage::KeepAlive(NoPayload)).await?,
                }

                // request next block
                if !waiting_for_piece && self.am_interested && !self.peer_choking {
                    if self
                        .req_next_block(&mut peer_writer)
                        .await
                        .context("requesting block")?
                    {
                        waiting_for_piece = true;
                    }
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
                peer_id: self.peer_id,
                msg,
            })
            .await
    }

    /// this sets our interested flag and sends the message to the peer
    async fn set_interested(
        &mut self,
        peer_writer: &mut PeerWriter,
        interested: bool,
    ) -> anyhow::Result<()> {
        if interested == self.am_interested {
            return Ok(());
        }
        self.am_interested = interested;

        let msg = if interested {
            PeerMessage::Interested(NoPayload)
        } else {
            PeerMessage::NotInterested(NoPayload)
        };
        peer_writer.send(msg).await.context("sending interested")?;
        Ok(())
    }

    /// this sets our choked flag and sends the message to the peer
    async fn set_choking(
        &mut self,
        peer_writer: &mut PeerWriter,
        choking: bool,
    ) -> anyhow::Result<()> {
        if choking == self.peer_choking {
            return Ok(());
        }
        self.am_choking = choking;
        let msg = if choking {
            PeerMessage::Choke(NoPayload)
        } else {
            PeerMessage::Unchoke(NoPayload)
        };
        peer_writer.send(msg).await.context("sending choked")?;
        Ok(())
    }

    async fn req_next_block(&mut self, peer_writer: &mut PeerWriter) -> anyhow::Result<bool> {
        if let Some(req) = self.req_queue.pop() {
            let req_msg = PeerMessage::Request(req);
            peer_writer.send(req_msg).await.context("Writing req")?;
            Ok(true)
        } else {
            self.send_req_manager(ReqMessage::NeedBlockQueue)
                .await
                .context("asking for new block-queue")?;
            Ok(false)
        }
    }

    async fn sever_conn(&mut self, peer_writer: &mut PeerWriter) -> bool {
        if self.peer_interested || self.am_interested {
            return false;
        }

        if peer_writer.close().await.is_err() {
            false
        } else {
            true
        }
    }
}
