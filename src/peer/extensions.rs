use std::boxed::Box;
use std::collections::HashMap;

use crate::{
    Peer,
    extensions::{
        BasicExtensionPayload, ExtensionAction, ExtensionHandler, ExtensionMessage, ExtensionType,
        factory::ExtensionFactory, protocol_extension_handshake::HandshakeExtension,
    },
    messages::PeerMessage,
    peer::error::PeerError,
    peer_manager::ReqMessage,
};

impl Peer {
    pub(super) async fn send_extended_handshake(&mut self) -> Result<(), PeerError> {
        if self.state.0.extensions.lock().unwrap().is_some() {
            let handshake_extension = HandshakeExtension::new();
            self.send_peer(PeerMessage::Extended(BasicExtensionPayload {
                extension_id: 0,
                data: serde_bencode::to_bytes(&handshake_extension)?,
            }))
            .await?;
        }
        Ok(())
    }

    pub(super) async fn on_extension_data(
        &mut self,
        payload: BasicExtensionPayload,
    ) -> Result<(), PeerError> {
        let actions_to_do = {
            let maybe_extensions = &mut *self.state.0.extensions.lock().unwrap();

            if let Some(extensions) = maybe_extensions {
                if payload.extension_id == ExtensionType::Handshake as u8 {
                    update_extensions(extensions, payload)?
                } else if let Some(extension) = extensions.get(&payload.extension_id) {
                    vec![extension.handle_message(&payload.data)]
                } else {
                    vec![]
                }
            } else {
                vec![]
            }
        };

        for action in actions_to_do {
            self.handle_action(action).await?;
        }
        Ok(())
    }

    async fn handle_action(&mut self, action: ExtensionAction) -> Result<(), PeerError> {
        match action {
            ExtensionAction::SendPeer(peer_message) => self.send_peer(peer_message).await,
            ExtensionAction::SendPeerManager(msg) => {
                // TODO: I update the self.queue.have_sent depending whether I received a real piece or a metadata piece on two different locations.
                // maybe put the queue inside a Mutex aswell and let the PeerManager update it
                if let ReqMessage::Extension(ExtensionMessage::ReceivedMetadataPiece {
                    piece_index: _,
                    data: _,
                }) = msg
                {
                    self.queue.have_sent -= 1;
                }

                self.send_peer_manager(msg).await
            }
            ExtensionAction::Nothing => Ok(()),
            ExtensionAction::Multiple(actions) => {
                for action in actions.into_iter() {
                    Box::pin(self.handle_action(action)).await?;
                }
                Ok(())
            }
        }
    }
}

fn update_extensions(
    extensions: &mut HashMap<u8, Box<dyn ExtensionHandler>>,
    payload: BasicExtensionPayload,
) -> Result<Vec<ExtensionAction>, PeerError> {
    let handshake = serde_bencode::from_bytes::<HandshakeExtension>(&payload.data)?;
    dbg!(&handshake);
    let mut actions = Vec::new();
    for (msg_type, msg_id) in handshake.m {
        if msg_id == 0 {
            extensions.remove(&msg_id);
        }
        let Some(new_extension) = ExtensionFactory::build(&msg_type) else {
            continue;
        };
        actions.push(new_extension.on_handshake(&handshake.other));
        extensions.insert(msg_id, new_extension);
    }

    Ok(actions)
}
