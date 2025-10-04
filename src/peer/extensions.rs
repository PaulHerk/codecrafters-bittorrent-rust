use crate::{
    Peer,
    extensions::{BasicExtensionPayload, handshake::HandshakeExtension},
    messages::PeerMessage,
    peer::error::PeerError,
};

impl Peer {
    pub(super) async fn send_extended_handshake(&mut self) -> Result<(), PeerError> {
        if self.state.0.extensions.lock().unwrap().is_some() {
            let handshake_extension = HandshakeExtension::new();
            self.send_peer(PeerMessage::Extended(BasicExtensionPayload {
                extension_id: 0,
                data: serde_bencode::to_bytes(&dbg!(handshake_extension))?,
            }))
            .await?;
        }
        Ok(())
    }
}
