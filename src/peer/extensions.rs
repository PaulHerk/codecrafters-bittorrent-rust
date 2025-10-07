use std::collections::HashMap;

use crate::{
    Peer,
    extensions::{
        BasicExtensionPayload, ExtensionAction, ExtensionHandler, ExtensionType,
        factory::ExtensionFactory, handshake::HandshakeExtension,
    },
    messages::PeerMessage,
    peer::error::PeerError,
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
                    let actions = update_extensions(extensions, payload)?;
                    actions
                } else if let Some(extension) = extensions.get(&payload.extension_id) {
                    vec![extension.handle_message(&payload.data)]
                } else {
                    vec![]
                }
            } else {
                vec![]
            }
        };

        dbg!(&actions_to_do);
        for action in actions_to_do {
            self.handle_action(action).await?;
        }
        Ok(())
    }

    async fn handle_action(&mut self, action: ExtensionAction) -> Result<(), PeerError> {
        match action {
            ExtensionAction::SendMessage(peer_message) => self.send_peer(peer_message).await,
            ExtensionAction::RequestToManager(extension_message) => {
                self.send_peer_manager(crate::peer_manager::ReqMessage::Extension(
                    extension_message,
                ))
                .await
            }
            ExtensionAction::Nothing => Ok(()),
        }
    }
}

fn update_extensions(
    extensions: &mut HashMap<u8, Box<dyn ExtensionHandler>>,
    payload: BasicExtensionPayload,
) -> Result<Vec<ExtensionAction>, PeerError> {
    let handshake = serde_bencode::from_bytes::<HandshakeExtension>(&payload.data)?;
    let mut actions = Vec::new();
    dbg!(&handshake);
    for (msg_type, msg_id) in handshake.m {
        if msg_id == 0 {
            extensions.remove(&msg_id);
        }
        let Some(new_extension) = ExtensionFactory::build(&msg_type) else {
            continue;
        };
        dbg!(&new_extension);
        actions.push(new_extension.on_handshake());
        extensions.insert(msg_id, new_extension);
    }

    Ok(actions)
}
