use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddrV4;
use tokio::net::TcpStream;
use tokio_util::codec::Framed;

use anyhow::Context;

use crate::messages::payloads::*;
use crate::messages::*;

pub mod handshake;
mod states;

pub struct Connection(Framed<TcpStream, MessageFramer>);

impl Connection {
    pub async fn connect(addr: SocketAddrV4) -> anyhow::Result<TcpStream> {
        tokio::net::TcpStream::connect(addr)
            .await
            .context("connect to peer")
    }

    pub async fn init_data_exchange(tcp: TcpStream) -> anyhow::Result<Self> {
        let mut framed = Framed::new(tcp, MessageFramer);
        let bitfield = framed
            .next()
            .await
            .expect("peer should send bitfield")
            .context("peer msg was invalid")?;
        let bitfield_idk = BitfieldPayload::from_be_bytes(&bitfield.payload);
        assert_eq!(bitfield.message_id, MessageType::Bitfield);
        // NOTE: we assume that all the peers have all the data

        let interested_msg = Message::new(MessageType::Interested, vec![]);
        framed
            .send(interested_msg)
            .await
            .context("write interested frame")?;

        let unchoke = framed
            .next()
            .await
            .expect("peer should send bitfield")
            .context("peer msg was invalid")?;
        assert_eq!(unchoke.message_id, MessageType::Unchoke);
        assert!(unchoke.payload.is_empty());

        Ok(Self(framed))
    }

    pub async fn get_block(
        &mut self,
        request_payload: RequestPiecePayload,
    ) -> anyhow::Result<ResponsePiecePayload> {
        let message = Message::new(MessageType::Request, request_payload.to_be_bytes());
        self.0.send(message).await.context("write request frame")?;
        let piece_msg = self
            .0
            .next()
            .await
            .expect("peer should send piece")
            .context("peer msg was invalid")?;
        assert_eq!(piece_msg.message_id, MessageType::Piece);
        assert!(piece_msg.payload.len() > 0);

        let response =
            ResponsePiecePayload::from_be_bytes(&piece_msg.payload, request_payload.length);
        assert_eq!(response.index, request_payload.index);
        assert_eq!(response.begin, request_payload.begin);
        Ok(response)
    }
}
