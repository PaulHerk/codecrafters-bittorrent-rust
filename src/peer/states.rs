use std::net::SocketAddrV4;

struct Peer {
    pub info_hash: [u8; 20],
    pub peer_id: [u8; 20],
    pub addr: SocketAddrV4,
    pub state: PeerState,
    pub choked_us: bool,
}

enum PeerState {
    Initial(InitialState),
    Core(CoreState),
    DataTransfer,
}
impl PeerState {
    pub fn new() -> Self {
        Self::Initial(InitialState::NotConnected)
    }
}

enum InitialState {
    NotConnected,
    Connected,
    Handshake,
}

/// Choked means that we choked the peer
enum CoreState {
    ChokedNotInterested,
    ChokedInterested,
    UnchokedInterested,
    /// after we downloaded all the pieces from a peer
    UnchokedNotInterested,
}
