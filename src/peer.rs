use std::net::SocketAddrV4;

use anyhow::Context;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub struct Connection {
    pub info_hash_self: [u8; 20],
    pub peer_id_self: [u8; 20],
    pub info_hash_other: [u8; 20],
    pub peer_id_other: [u8; 20],
    pub tcp: tokio::net::TcpStream,
}

impl Connection {
    pub async fn new(
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
        Ok(Self {
            info_hash_self: *info_hash_self,
            peer_id_self: *peer_id_self,
            info_hash_other,
            peer_id_other,
            tcp,
        })
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
