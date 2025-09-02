use futures_util::stream::{SplitSink, unfold};
use futures_util::{self, SinkExt, StreamExt};
use std::net::SocketAddrV4;
use tokio::sync::broadcast::{Receiver, Sender};
use tokio_util::codec::Framed;

use anyhow::Context;

use crate::messages::payloads::*;
use crate::states::PeerState;
use crate::{messages::*, *};

pub mod handshake;
pub mod peer_data;
pub mod states;

#[derive(Debug, Clone)]
pub struct Peer {
    pub peer_id: [u8; 20],
    pub addr: SocketAddrV4,
    pub state: PeerState,
    pub am_choking: bool,
    pub am_interested: bool,
    pub peer_choking: bool,
    pub peer_interested: bool,
    pub has: Vec<bool>,
}

/// this enum is used to select between different stream-types
#[derive(Debug)]
enum Msg {
    /// this is sent by other peers in order for this particular peer to announce that it has the piece
    HavePayload(HavePayload),
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
        }
    }
    /// info_hash: the info hash of the torrent
    /// peer_id: a unique identifier for your client
    pub async fn shake_hands_get_framed(
        &mut self,
        info_hash: [u8; 20],
    ) -> anyhow::Result<MsgFrameType> {
        let mut tcp = tokio::net::TcpStream::connect(self.addr)
            .await
            .context("connect to peer")?;
        self.state = PeerState::Connected;

        let handshake_to_send = Handshake::new(info_hash, self.peer_id);
        let handshake_recv = handshake_to_send
            .shake_hands(&mut tcp)
            .await
            .context("peer should send handshake")?;

        assert_eq!(handshake_recv.info_hash, info_hash); // TODO: sever connection
        // assert_eq!(handshake_recv.peer_id, self.peer_id);

        self.state = PeerState::DataTransfer;

        let framed = Framed::new(tcp, MessageFramer);

        println!("peer {} connected", self.addr);
        Ok(framed)
    }

    async fn request_piece(
        &mut self,
        peer_writer: &mut SplitSink<MsgFrameType, Message>,
        peer_data: &PeerData,
    ) -> anyhow::Result<bool> {
        let req = peer_data.prepare_next_req_send(&self.has);
        if let Some(req) = req {
            peer_writer
                .send(Message::new(MessageAll::Request(req)))
                .await
                .context("send request")?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub async fn event_loop(
        &mut self,
        framed: MsgFrameType,
        peer_data: PeerData,
        (broadcast_tx, broadcast_rx): (Sender<HavePayload>, Receiver<HavePayload>),
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
        let have_stream = unfold(broadcast_rx, |mut rx| async move {
            let Ok(have_payload) = rx.recv().await else {
                return None;
            };
            Some((Msg::HavePayload(have_payload), rx))
        });
        tokio::pin!(have_stream);

        let duration = std::time::Duration::from_secs(120);
        let timeout = tokio::time::interval_at(tokio::time::Instant::now() + duration, duration);
        let timeout_stream = unfold(timeout, |mut timeout| async move {
            timeout.tick().await;
            Some((Msg::Timeout, timeout))
        });
        tokio::pin!(timeout_stream);

        let mut stream = futures_util::stream_select!(peer_msg_stream, have_stream, timeout_stream);

        loop {
            if let Some(message) = stream.next().await {
                match message {
                    Msg::HavePayload(have_payload) => {
                        peer_writer
                            .send(Message::new(MessageAll::Have(have_payload)))
                            .await
                            .context("send have")?;
                        eprintln!("sent have");
                    }
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
                                // TODO: we're interested by default for now

                                let interested_msg: Message =
                                    Message::new(MessageAll::Interested(NoPayload));
                                peer_writer
                                    .send(interested_msg)
                                    .await
                                    .context("write interested frame")?;
                                let _ = self.request_piece(&mut peer_writer, &peer_data).await?;

                                self.am_interested = true;
                            }
                            MessageAll::Request(request_piece_payload) => {
                                let block = peer_data.get_block(request_piece_payload);
                                if let Some(block) = block {
                                    let response_piece_payload = ResponsePiecePayload {
                                        index: request_piece_payload.index,
                                        begin: request_piece_payload.begin,
                                        block,
                                    };
                                    peer_writer
                                        .send(Message::new(MessageAll::Piece(
                                            response_piece_payload,
                                        )))
                                        .await
                                        .context("send response")?;
                                } else {
                                    println!("all done.")
                                    // TODO: I'm not sure if we should send something if the peer sent us a request for
                                    // either: a piece that we don't have
                                    // or: just an invalid request
                                }
                            }
                            MessageAll::Piece(response_piece_payload) => {
                                if let Some(index) = peer_data.add_block(response_piece_payload) {
                                    let have_payload = HavePayload { piece_index: index };
                                    broadcast_tx.send(have_payload).unwrap();
                                }
                                if !self.request_piece(&mut peer_writer, &peer_data).await? {
                                    continue;
                                    // let's pretend we are done
                                    // realistically, we should be seeding now
                                    // set interested to false
                                }
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
            }
        }
    }
}
