/// This is all for the peer itself
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

use crate::extensions::{
    ExtensionAction, ExtensionHandler, ExtensionMessage,
    protocol_extension_handshake::AdditionalHandshakeInfo,
};

#[derive(Debug)]
pub(crate) struct MetadataRequester;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct MetadataMsg {
    pub(super) msg_type: MetadataMsgType,
    #[serde(rename = "piece")]
    pub(super) piece_index: u32,
    pub(super) total_size: Option<u32>,
}

#[derive(Debug, PartialEq, Deserialize_repr, Serialize_repr)]
#[repr(u8)]
pub(super) enum MetadataMsgType {
    Request = 0,
    Data = 1,
    Reject = 2,
    Other,
}

impl MetadataRequester {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl ExtensionHandler for MetadataRequester {
    fn handle_message(&self, data: &[u8]) -> ExtensionAction {
        let Ok(msg): Result<MetadataMsg, _> = serde_bencode::from_bytes(data) else {
            return ExtensionAction::Nothing;
        };
        let Some(total_size) = msg.total_size else {
            return ExtensionAction::Nothing;
        };
        let Some(offset) = data.len().checked_sub(total_size as usize) else {
            return ExtensionAction::Nothing;
        };
        let data = data[offset..].to_vec();
        ExtensionAction::SendPeerManager(ExtensionMessage::ReceivedMetadataPiece {
            piece_index: msg.piece_index,
            data,
        })
    }

    fn on_handshake(&self, additional_info: &AdditionalHandshakeInfo) -> ExtensionAction {
        let Some(length) = additional_info.metadata_size else {
            return ExtensionAction::SendPeerManager(ExtensionMessage::NeedMetadataPiece);
        };
        let requests = vec![
            ExtensionAction::SendPeerManager(ExtensionMessage::GotMetadataLength(length)),
            ExtensionAction::SendPeerManager(ExtensionMessage::NeedMetadataPiece),
        ];
        ExtensionAction::Multiple(requests)
    }

    fn get_ext_type(&self) -> crate::extensions::ExtensionType {
        crate::extensions::ExtensionType::Metadata
    }
}

#[cfg(test)]
mod test_metadata_msg {
    use super::*;

    #[test]
    fn test_req() {
        let msg = MetadataMsg {
            msg_type: MetadataMsgType::Request,
            piece_index: 0,
            total_size: None,
        };
        assert_eq!(
            serde_bencode::to_string(&msg).unwrap(),
            "d8:msg_typei0e5:piecei0ee"
        );
        assert_eq!(
            serde_bencode::from_str::<MetadataMsg>("d8:msg_typei0e5:piecei0ee").unwrap(),
            msg
        );
    }

    #[test]
    fn test_data() {
        let metadata_requester = MetadataRequester::new();
        let bytes = b"d8:msg_typei1e5:piecei0e10:total_sizei8eexxxxxxxx";
        let action = metadata_requester.handle_message(bytes);
        assert_eq!(
            action,
            ExtensionAction::SendPeerManager(ExtensionMessage::ReceivedMetadataPiece {
                piece_index: 0,
                data: b"xxxxxxxx".to_vec()
            })
        );
    }
}
