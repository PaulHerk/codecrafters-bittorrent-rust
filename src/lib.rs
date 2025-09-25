pub mod database;
pub mod messages;
pub mod peer;
mod req_manager;
pub mod torrent;
pub mod tracker;

use std::collections::HashMap;

pub use database::*;
pub use messages::payloads::*;
pub use messages::*;
pub use peer::handshake::*;
// pub use peer::peer_data::*;
pub use peer::states::*;
pub use peer::*;
pub use req_manager::*;
use tokio_util::codec::Framed;
pub use torrent::*;
pub use tracker::*;

pub const BLOCK_MAX: u32 = 1 << 14;

pub type MsgFrameType = Framed<tokio::net::TcpStream, MessageFramer>;

pub struct BittorrentClient {
    pub torrents: Vec<Torrent>,
    pub req_manager: HashMap<String, ReqManager>,
    //
    // pub peers: HashMap<MsgFrameType, (Peer, Framed<TcpStream, MessageFramer>)>,
}
