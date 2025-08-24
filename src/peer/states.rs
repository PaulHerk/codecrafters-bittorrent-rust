#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerState {
    /// before anything happened
    NotConnected,
    /// if tcp connection is established
    Connected,
    /// after handshake succeeded
    DataTransfer,
}
impl Default for PeerState {
    fn default() -> Self {
        PeerState::NotConnected
    }
}

// #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
// pub enum InitialState {
//     #[default]
//     NotConnected,
//     Connected,
// }

// /// After the handshake succeeded, we are in the core state with coked and not interested
// /// Choked means that we choked the peer
// #[derive(Debug, Clone, Copy, PartialEq, Eq)]
// pub enum CoreState {
//     ChokedNotInterested = 0,
//     ChokedInterested = 1,
//     UnchokedInterested = 2,
//     /// after we downloaded all the pieces from a peer
//     UnchokedNotInterested = 3,
// }
