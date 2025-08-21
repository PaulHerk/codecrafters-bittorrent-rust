pub mod messages;
pub mod peer;
pub mod torrent;
pub mod tracker;

pub use self::messages::payloads::*;
pub use self::messages::*;
pub use self::peer::*;
pub use self::torrent::*;
pub use self::tracker::*;
pub use peer::handshake::*;

pub const BLOCK_MAX: u32 = 1 << 14;

pub fn escape_bytes_url(bytes: &[u8; 20]) -> String {
    bytes
        .iter()
        .map(|b| format!("%{}", hex::encode([*b])))
        .collect()
}
