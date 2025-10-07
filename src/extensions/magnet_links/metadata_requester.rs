use crate::extensions::{ExtensionAction, ExtensionHandler, ExtensionMessage};

#[derive(Debug)]
pub(crate) struct MetadataRequester;

impl MetadataRequester {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl ExtensionHandler for MetadataRequester {
    fn handle_message(&self, data: &[u8]) -> ExtensionAction {
        todo!()
    }

    fn on_handshake(&self) -> ExtensionAction {
        ExtensionAction::RequestToManager(ExtensionMessage::NeedMetadataPiece)
    }
}
