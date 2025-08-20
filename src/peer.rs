use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddrV4;
use tokio_util::codec::Framed;

use anyhow::Context;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::messages::{
    Message, MessageFramer, MessageType, RequestPieceMsgPayload, ResponsePieceMsgPayload,
};

pub struct Connection {
    pub info_hash_self: [u8; 20],
    pub peer_id_self: [u8; 20],
    pub info_hash_other: [u8; 20],
    pub peer_id_other: [u8; 20],
    pub framed_read_write: Framed<tokio::net::TcpStream, MessageFramer>,
}

impl Connection {
    pub async fn connect(
        info_hash_self: &[u8; 20],
        peer_id_self: &[u8; 20],
        addr: SocketAddrV4,
    ) -> anyhow::Result<Self> {
        let mut tcp = tokio::net::TcpStream::connect(addr)
            .await
            .context("connect to peer")?;
        write_handshake(info_hash_self, peer_id_self, &mut tcp)
            .await
            .context("write handshake")?;
        let (info_hash_other, peer_id_other) =
            read_handshake(&mut tcp).await.context("read handshake")?;
        let framed = Framed::new(tcp, MessageFramer);
        Ok(Self {
            info_hash_self: *info_hash_self,
            peer_id_self: *peer_id_self,
            info_hash_other,
            peer_id_other,
            framed_read_write: framed,
        })
    }

    pub async fn init_data_exchange(&mut self) -> anyhow::Result<()> {
        let bitfield = self
            .framed_read_write
            .next()
            .await
            .expect("peer should send bitfield")
            .context("peer msg was invalid")?;
        assert_eq!(bitfield.message_id, MessageType::Bitfield);
        // NOTE: we assume that all the peers have all the data

        let interested_msg = Message::new(MessageType::Interested, vec![]);
        self.framed_read_write
            .send(interested_msg)
            .await
            .context("write interested frame")?;

        let unchoke = self
            .framed_read_write
            .next()
            .await
            .expect("peer should send bitfield")
            .context("peer msg was invalid")?;
        assert_eq!(unchoke.message_id, MessageType::Unchoke);
        assert!(unchoke.payload.is_empty());

        Ok(())
    }

    pub async fn get_block(
        &mut self,
        request_payload: RequestPieceMsgPayload,
    ) -> anyhow::Result<ResponsePieceMsgPayload> {
        let message = Message::new(MessageType::Request, request_payload.to_be_bytes());
        self.framed_read_write
            .send(message)
            .await
            .context("write request frame")?;
        let piece_msg = self
            .framed_read_write
            .next()
            .await
            .expect("peer should send piece")
            .context("peer msg was invalid")?;
        assert_eq!(piece_msg.message_id, MessageType::Piece);
        assert!(piece_msg.payload.len() > 0);

        let response =
            ResponsePieceMsgPayload::from_be_bytes(&piece_msg.payload, request_payload.length);
        assert_eq!(response.index, request_payload.index);
        assert_eq!(response.begin, request_payload.begin);
        Ok(response)
    }
}

async fn write_handshake(
    info_hash: &[u8; 20],
    peer_id: &[u8; 20],
    tcp: &mut tokio::net::TcpStream,
) -> anyhow::Result<()> {
    let mut buf = [0u8; 68];
    buf[0..1].copy_from_slice(&[19]);
    buf[1..20].copy_from_slice(b"BitTorrent protocol");
    buf[20..28].copy_from_slice(b"00000000");
    buf[28..48].copy_from_slice(info_hash);
    buf[48..68].copy_from_slice(peer_id);
    tcp.write_all(&buf).await.context("write handshake")?;
    Ok(())
}

async fn read_handshake(tcp: &mut tokio::net::TcpStream) -> anyhow::Result<([u8; 20], [u8; 20])> {
    let mut buf = [0u8; 68];
    tcp.read_exact(&mut buf).await.context("read handshake")?;
    let length = &buf[0];
    let mut protocol = String::new();
    (&buf[1..20])
        .read_to_string(&mut protocol)
        .await
        .context("read protocol")?;
    let reserved = &buf[20..28];
    if length != &19 || protocol != "BitTorrent protocol"
    /* || reserved != b"00000000" */ // currently it returns 00000004 for some reason
    {
        return Err(anyhow::anyhow!(
            "invalid protocol, {:?} {:?} {:?}",
            length,
            protocol,
            reserved
        ));
    }
    let info_hash: [u8; 20] = buf[28..48].try_into().context("convert to [u8; 20]")?;
    let peer_id: [u8; 20] = buf[48..68].try_into().context("convert to [u8; 20]")?;

    Ok((info_hash, peer_id))
}
