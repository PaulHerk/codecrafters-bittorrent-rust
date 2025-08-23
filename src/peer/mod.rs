use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddrV4;
use tokio::net::TcpStream;
use tokio_util::codec::Framed;

use anyhow::Context;

use crate::messages::payloads::*;
use crate::{messages::*, Handshake, InitialState, MsgFrameType, PeerState};

pub mod handshake;
pub mod states;

#[derive(Debug, Clone)]
pub struct Peer {
    pub peer_id: [u8; 20],
    pub addr: SocketAddrV4,
    pub state: PeerState,
    pub choked_us: bool,
}

impl Peer {
    pub fn new(peer_id: [u8; 20], addr: SocketAddrV4) -> Self {
        Self {
            peer_id,
            addr,
            state: PeerState::default(),
            choked_us: false,
        }
    }
    /// info_hash: the info hash of the torrent
    /// peer_id: a unique identifier for your client
    pub async fn shake_hands_get_framed(
        &mut self,
        info_hash: [u8; 20],
    ) -> anyhow::Result<MsgFrameType> {
        let mut tcp = Connection::connect(self.addr)
            .await
            .context("connect to peer")?;
        self.state = PeerState::Initial(InitialState::Connected);

        let handshake_to_send = Handshake::new(info_hash, self.peer_id);
        let handshake_recv = handshake_to_send
            .shake_hands(&mut tcp)
            .await
            .expect("peer should send handshake");

        assert_eq!(handshake_recv.info_hash, info_hash);
        // assert_eq!(handshake_recv.peer_id, self.peer_id);

        self.state = PeerState::Initial(InitialState::Handshake);

        let framed = Framed::new(tcp, MessageFramer);
        Ok(framed)
    }
}

pub struct Connection(Framed<TcpStream, MessageFramer>);

impl Connection {
    pub async fn connect(addr: SocketAddrV4) -> anyhow::Result<TcpStream> {
        tokio::net::TcpStream::connect(addr)
            .await
            .context("connect to peer")
    }

    pub async fn init_data_exchange(mut framed: MsgFrameType) -> anyhow::Result<Self> {
        let bitfield: Message = framed
            .next()
            .await
            .expect("peer should send bitfield")
            .context("peer msg was invalid")?;
        assert_eq!(bitfield.get_msg_type(), 5);
        // NOTE: we assume that all the peers have all the data

        let interested_msg: Message = Message::new(MessageAll::Interested(NoPayload));
        framed
            .send(interested_msg)
            .await
            .context("write interested frame")?;

        let unchoke: Message = framed
            .next()
            .await
            .expect("peer should send bitfield")
            .context("peer msg was invalid")
            .unwrap();
        assert_eq!(unchoke.get_msg_type(), 1);
        // assert!(unchoke.payload.is_empty());

        Ok(Self(framed))
    }

    pub async fn get_block(
        &mut self,
        request_payload: RequestPiecePayload,
    ) -> anyhow::Result<ResponsePiecePayload> {
        let message = Message::new(MessageAll::Request(request_payload));
        self.0.send(message).await.context("write request frame")?;
        let piece_msg: Message = self
            .0
            .next()
            .await
            .expect("peer should send piece")
            .context("peer msg was invalid")?;
        let MessageAll::Piece(payload) = piece_msg.payload else {
            unreachable!()
        };
        assert!(payload.block.len() > 0);

        assert_eq!(payload.index, request_payload.index);
        assert_eq!(payload.begin, request_payload.begin);
        Ok(payload)
    }
}
