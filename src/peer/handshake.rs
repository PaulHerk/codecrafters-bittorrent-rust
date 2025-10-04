use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::{peer::error::PeerError, torrent::InfoHash};

#[derive(Debug, Copy, Clone, bincode::Encode, bincode::Decode)]
pub struct Handshake {
    length: u8,
    protocol: [u8; 19],
    reserved: [u8; 8],
    pub info_hash: [u8; 20],
    pub peer_id: [u8; 20],
}
const HANDSHAKE_LEN: usize = std::mem::size_of::<Handshake>();

impl Handshake {
    pub fn new(info_hash: InfoHash, peer_id: [u8; 20]) -> Self {
        let mut reserved = [0_u8; 8];

        // The bit selected for the extension protocol is bit 20 from the right
        // .... 00010000 00000000 00000000
        //         ^ 20th bit from the right, counting starts at 0
        reserved[5] = 0x10;
        Self {
            length: 19,
            protocol: *b"BitTorrent protocol",
            reserved,
            info_hash: info_hash.0,
            peer_id,
        }
    }

    /// Initializes the handshake by writing the handshake to the tcp stream
    /// and returning the handshake received from the tcp stream
    pub async fn shake_hands(
        self,
        tcp: &mut tokio::net::TcpStream,
    ) -> Result<Handshake, PeerError> {
        let config = bincode::config::standard()
            .with_big_endian()
            .with_limit::<HANDSHAKE_LEN>();
        // write handshake
        let mut handshake_bytes = [0_u8; HANDSHAKE_LEN];
        bincode::encode_into_slice(self, &mut handshake_bytes, config)?;
        tcp.write_all(&handshake_bytes)
            .await
            .map_err(|error| PeerError::SendToPeer {
                error,
                peer_id: self.peer_id,
                msg_type_str: "Handshake".to_string(),
            })?;

        // read handshake
        handshake_bytes = [0_u8; 68];
        tcp.read_exact(&mut handshake_bytes)
            .await
            .map_err(PeerError::RecvHandshake)?;

        let (handshake_recv, len) =
            bincode::decode_from_slice::<Handshake, _>(&handshake_bytes, config)?;

        assert_eq!(len, HANDSHAKE_LEN);
        assert_eq!(handshake_recv.length, 19);
        assert_eq!(handshake_recv.protocol, *b"BitTorrent protocol");
        assert_eq!(handshake_recv.info_hash, self.info_hash);
        Ok(handshake_recv)
    }

    pub fn has_extensions_enabled(&self) -> bool {
        let extension_bit = self.reserved[5] & 0x10;
        extension_bit == 0x10
    }
}
