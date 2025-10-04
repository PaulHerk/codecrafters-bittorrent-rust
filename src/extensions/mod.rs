use bytes::BufMut;
use serde::{Deserialize, Serialize};

use crate::messages::payloads::Payload;
pub(crate) mod handshake;
// pub mod magnet_links;

/// This Payload is merely a holder for the extended message ID and data.
/// We can build a full ExtensionPayload directly from the peer since we need the HashMap of the extensions
/// which we don't have here
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BasicExtensionPayload {
    pub(crate) extension_id: u8,
    pub(crate) data: Vec<u8>,
}

impl Payload for BasicExtensionPayload {
    fn from_be_bytes(data: &[u8]) -> Self {
        let extension_id = u8::from_be(data[0]);
        let data = data[1..].to_vec();
        Self { extension_id, data }
    }

    fn to_be_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.data.len() + 1);
        bytes.put_u8(self.extension_id);
        bytes.extend_from_slice(&self.data);
        bytes
    }
}

/// Enum that represents the currently supportet Extensions
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Hash)]
pub(crate) enum ExtensionType {
    Handshake,
    Metadata,
}

// the handshake is always active
pub const ACTIVE_EXTENSIONS: &[ExtensionType] = &[ExtensionType::Metadata];
