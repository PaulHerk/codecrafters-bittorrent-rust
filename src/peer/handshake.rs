use anyhow::Context;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

#[derive(Debug, Copy, Clone, bincode::Encode, bincode::Decode)]
pub struct Handshake {
    length: u8,
    protocol: [u8; 19],
    reserved: [u8; 8],
    pub info_hash: [u8; 20],
    pub peer_id: [u8; 20],
}
impl Handshake {
    pub fn new(info_hash: [u8; 20], peer_id: [u8; 20]) -> Self {
        Self {
            length: 19,
            protocol: *b"BitTorrent protocol",
            reserved: *b"00000000",
            info_hash,
            peer_id,
        }
    }

    /// Initializes the handshake by writing the handshake to the tcp stream
    /// and returning the handshake received from the tcp stream
    pub async fn init_handshake(self, tcp: &mut tokio::net::TcpStream) -> anyhow::Result<Self> {
        let config = bincode::config::standard()
            .with_big_endian()
            .with_limit::<68>();
        // write handshake
        let mut handshake_bytes = [0_u8; 68];
        bincode::encode_into_slice(self, &mut handshake_bytes, config)
            .context("encode handshake")?;
        tcp.write_all(&handshake_bytes)
            .await
            .context("write handshake")?;

        // read handshake
        handshake_bytes = [0_u8; 68];
        tcp.read_exact(&mut handshake_bytes)
            .await
            .context("read handshake")?;
        let (handshake_recv, len) =
            bincode::decode_from_slice::<Handshake, _>(&handshake_bytes, config)
                .context("decode handshake")?;

        assert_eq!(len, 68);
        assert_eq!(handshake_recv.length, 19);
        assert_eq!(handshake_recv.protocol, *b"BitTorrent protocol");
        // assert_eq!(handshake_recv.reserved, [0_u8; 8]); // somehow the server sends 00000004
        Ok(handshake_recv)
    }
}
