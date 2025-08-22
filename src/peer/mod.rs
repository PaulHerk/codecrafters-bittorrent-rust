use futures_util::{SinkExt, StreamExt};
use std::marker::PhantomData;
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
        let bitfield: Message = framed
            .next()
            .await
            .expect("peer should send bitfield")
            .context("peer msg was invalid")?;
        assert_eq!(bitfield.get_msg_type(), 5);
        // NOTE: we assume that all the peers have all the data

        let interested_msg: Message = Message::new(MessageAll::Interested(P(NoPayload)));
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
        let message = Message::new(MessageAll::Request(P(request_payload)));
        self.0.send(message).await.context("write request frame")?;
        let piece_msg: Message = self
            .0
            .next()
            .await
            .expect("peer should send piece")
            .context("peer msg was invalid")?;
        let MessageAll::Piece(P(payload)) = piece_msg.payload else {
            unreachable!()
        };
        assert!(payload.block.len() > 0);

        assert_eq!(payload.index, request_payload.index);
        assert_eq!(payload.begin, request_payload.begin);
        Ok(payload)
    }
}
