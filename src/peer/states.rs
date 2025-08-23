#[derive(Debug, Clone, Copy)]
pub enum PeerState {
    Initial(InitialState),
    Core(CoreState),
    DataTransfer,
}
impl Default for PeerState {
    fn default() -> Self {
        PeerState::Initial(InitialState::default())
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub enum InitialState {
    #[default]
    NotConnected,
    Connected,
    Handshake,
}

#[derive(Debug, Clone, Copy)]
/// Choked means that we choked the peer
pub enum CoreState {
    ChokedNotInterested,
    ChokedInterested,
    UnchokedInterested,
    /// after we downloaded all the pieces from a peer
    UnchokedNotInterested,
}
