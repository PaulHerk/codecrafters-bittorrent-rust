pub mod messages;
pub mod peer;
pub mod torrent;
pub mod tracker;

use std::collections::HashMap;

pub use self::messages::payloads::*;
pub use self::messages::*;
pub use self::peer::*;
pub use self::torrent::*;
pub use self::tracker::*;
pub use peer::handshake::*;
pub use peer::states::*;
use tokio::net::TcpStream;
use tokio_util::codec::Framed;

pub const BLOCK_MAX: u32 = 1 << 14;

pub type MsgFrameType = Framed<tokio::net::TcpStream, MessageFramer>;

pub struct BittorrentClient {
    pub torrent: Torrent,
    pub peers: HashMap<MsgFrameType, (Peer, Framed<TcpStream, MessageFramer>)>,
}
