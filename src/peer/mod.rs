use futures_util::stream::{SplitSink, unfold};
use futures_util::{self, SinkExt, StreamExt};
use std::net::SocketAddrV4;
use tokio::sync::{mpsc, oneshot};
use tokio_util::codec::Framed;

use anyhow::Context;

use crate::messages::payloads::*;
use crate::states::PeerState;
use crate::{messages::*, *};

pub mod handshake;
// pub mod peer_data;
pub mod states;

type PeerWriter = SplitSink<Framed<TcpStream, MessageFramer>, Message>;

#[derive(Debug, Clone)]
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
    pub req_queue: Option<Vec<RequestPiecePayload>>,
}

/// this enum is used to select between different stream-types a peer can receive
#[derive(Debug)]
enum Msg {
    /// this will be sent to other peers in order to announce that it has the piece
    ManagerMsg(PeerMsg),
    Data(Message),
    Timeout,
}

impl Peer {
    pub fn new(peer_id: [u8; 20], addr: SocketAddrV4) -> Self {
        Self {
            peer_id,
            addr,
            state: PeerState::default(),
            am_choking: true,
            am_interested: false,
            peer_choking: true,
            peer_interested: false,
            has: Vec::new(),
            req_queue: None,
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

    pub async fn event_loop(
        &mut self,
        framed: MsgFrameType,
        manager_tx: mpsc::Sender<ReqMessage>,
        // used for sending have messages to the peer
        has_receiver: tokio::sync::broadcast::Receiver<PeerMsg>,
    ) -> anyhow::Result<()> {
        assert_eq!(self.state, PeerState::DataTransfer);
        // TODO: check if we are interested or not
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
        let manager_stream = unfold(has_receiver, |mut rx| async move {
            let Ok(msg) = rx.recv().await else {
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
        loop {
            if let Some(message) = stream.next().await {
                match message {
                    Msg::ManagerMsg(peer_msg) => match peer_msg {
                        PeerMsg::Shutdown => {
                            todo!("shutdown peer")
                        }
                        PeerMsg::Have(have_payload) => {
                            peer_writer
                                .send(Message::new(MessageAll::Have(have_payload)))
                                .await
                                .context("send have")?;
                            eprintln!("sent have");
                        }
                    },
                    Msg::Data(message) => {
                        match message.payload {
                            MessageAll::Choke(_no_payload) => {
                                self.peer_choking = true;
                                return Err(anyhow::anyhow!("peer is choked"));
                            }
                            MessageAll::Unchoke(_no_payload) => self.peer_choking = false,
                            MessageAll::Interested(_no_payload) => self.peer_interested = true,
                            MessageAll::NotInterested(_no_payload) => self.peer_interested = false,
                            MessageAll::Have(have_payload) => {
                                self.has[have_payload.piece_index as usize] = true
                            }
                            MessageAll::Bitfield(bitfield_payload) => {
                                self.has = bitfield_payload.pieces_available;
                                self.update_req_queue(&manager_tx).await?;
                                // TODO: we're interested by default for now

                                if self.req_queue.is_some() {
                                    let interested_msg: Message =
                                        Message::new(MessageAll::Interested(NoPayload));
                                    peer_writer
                                        .send(interested_msg)
                                        .await
                                        .context("write interested frame")?;
                                    self.req_next_block(&mut peer_writer, &manager_tx).await?;
                                }

                                self.am_interested = true;
                            }
                            MessageAll::Request(request_piece_payload) => {
                                let (tx, rx) = oneshot::channel();

                                let msg = ReqMessage::NeedBlock {
                                    block: request_piece_payload,
                                    tx,
                                };
                                manager_tx.send(msg).await?;
                                let block = rx.await;
                                if let Ok(response_piece_payload) = block {
                                    peer_writer
                                        .send(Message::new(MessageAll::Piece(
                                            response_piece_payload,
                                        )))
                                        .await
                                        .context("send response")?;
                                } else {
                                    // TODO: I'm not sure if we should send something if the peer sent us a request for
                                    // either: a piece that we don't have
                                    // or: just an invalid request
                                }
                            }
                            MessageAll::Piece(response_piece_payload) => {
                                let msg = ReqMessage::GotBlock {
                                    block: response_piece_payload,
                                };
                                manager_tx.send(msg).await?;

                                self.req_next_block(&mut peer_writer, &manager_tx).await?;
                            }
                            MessageAll::Cancel(_request_piece_payload) => todo!(), // only for extension, won't probably use it
                            MessageAll::KeepAlive(_no_payload) => eprintln!("he sent a keep alive"),
                        }
                    }
                    Msg::Timeout => {
                        peer_writer
                            .send(Message::new(MessageAll::KeepAlive(NoPayload)))
                            .await?
                    }
                }
            } else {
                eprintln!("peer disconnected");
                break Err(anyhow::anyhow!("peer disconnected"));
            }
        }
    }

    async fn req_next_block(
        &mut self,
        peer_writer: &mut PeerWriter,
        manager_tx: &mpsc::Sender<ReqMessage>,
    ) -> anyhow::Result<()> {
        let Some(queue) = &mut self.req_queue else {
            return Ok(());
        };
        if queue.is_empty() {
            let (tx, rx) = oneshot::channel();
            let msg = ReqMessage::NeedBlocksToReq {
                peer_has: self.has.clone(),
                tx,
            };
            manager_tx.send(msg).await?;
            *queue = rx.await?;
        }
        if let Some(req) = queue.pop() {
            let req_msg = Message::new(MessageAll::Request(req));
            peer_writer.send(req_msg).await.context("Writing req")?;
        } else {
            // TODO: set interested to false
        }

        Ok(())
    }

    async fn update_req_queue(
        &mut self,
        manager_tx: &mpsc::Sender<ReqMessage>,
    ) -> anyhow::Result<()> {
        let new_queue = self.get_new_req_queue(&manager_tx).await?;
        self.req_queue = new_queue;

        Ok(())
    }

    async fn get_new_req_queue(
        &self,
        manager_tx: &mpsc::Sender<ReqMessage>,
    ) -> anyhow::Result<Option<Vec<RequestPiecePayload>>> {
        let (tx, rx) = oneshot::channel();
        let req = ReqMessage::NeedBlocksToReq {
            peer_has: self.has.clone(),
            tx,
        };
        manager_tx.send(req).await?;

        let res = rx.await;
        if let Ok(res) = res {
            Ok(Some(res))
        } else {
            // sever the connection
            Ok(None)
        }
    }

    async fn sever_conn(&mut self) {
        todo!()
    }
}
