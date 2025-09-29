mod database;
mod messages;
mod peer;
mod peer_manager;
mod torrent;
mod tracker;

pub use peer::Peer;
pub use peer_manager::PeerManager;
use std::collections::HashMap;
pub use torrent::Torrent;
pub use tracker::TrackerRequest;

pub(crate) const BLOCK_MAX: u32 = 1 << 14;

// pub(crate) type MsgFrameType = Framed<tokio::net::TcpStream, MessageFramer>;

pub struct BittorrentClient {
    pub torrents: Vec<Torrent>,
    pub peer_manager: HashMap<String, PeerManager>,
    //
    // pub peers: HashMap<MsgFrameType, (Peer, Framed<TcpStream, MessageFramer>)>,
}
