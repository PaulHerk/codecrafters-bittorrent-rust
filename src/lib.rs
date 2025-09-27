mod database;
mod messages;
mod peer;
mod req_manager;
mod torrent;
mod tracker;

pub use peer::conn::Peer;
pub use req_manager::ReqManager;
use std::collections::HashMap;
pub use torrent::Torrent;
pub use tracker::TrackerRequest;

pub(crate) const BLOCK_MAX: u32 = 1 << 14;

// pub(crate) type MsgFrameType = Framed<tokio::net::TcpStream, MessageFramer>;

pub struct BittorrentClient {
    pub torrents: Vec<Torrent>,
    pub req_manager: HashMap<String, ReqManager>,
    //
    // pub peers: HashMap<MsgFrameType, (Peer, Framed<TcpStream, MessageFramer>)>,
}
