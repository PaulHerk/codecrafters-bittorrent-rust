use bytes::BufMut;
use strum::{Display, EnumString};

use crate::{
    extensions::handshake::AdditionalHandshakeInfo,
    messages::{PeerMessage, payloads::Payload},
};
pub(crate) mod factory;
pub(crate) mod handshake;
pub mod magnet_links;

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

/// Represents an action that the Peer should take after an extension message is handled.
#[derive(Debug, Clone, PartialEq)]
pub enum ExtensionAction {
    /// Send a message back to the remote peer.
    SendPeer(PeerMessage),
    /// Send a request to the PeerManager.
    SendPeerManager(ExtensionMessage),
    /// Multiple actions
    Multiple(Vec<ExtensionAction>),
    /// Do nothing.
    Nothing,
}

/// A message type specifically for communication from an extension to the PeerManager.
#[derive(Clone, Debug, PartialEq)]
pub enum ExtensionMessage {
    NeedMetadataPiece,
    ReceivedMetadataPiece { piece_index: u32, data: Vec<u8> },
    GotMetadataLength(usize),
}

/// The core trait for handling extension-specific logic.
pub trait ExtensionHandler: Send + Sync + std::fmt::Debug {
    /// Called when a new extension message is received from the peer.
    /// It takes the raw payload of the extension message.
    fn handle_message(&self, data: &[u8]) -> ExtensionAction;

    /// Called periodically or on specific events (like the initial handshake).
    /// This allows the extension to proactively send messages.
    /// For example, the metadata extension could use this to start requesting pieces.
    fn on_handshake(&self, additional_info: &AdditionalHandshakeInfo) -> ExtensionAction;

    fn get_ext_type(&self) -> ExtensionType;
}

/// Enum that represents the currently supportet Extensions
#[derive(Debug, Display, EnumString, Clone, Copy, PartialEq, Eq, PartialOrd, Hash)]
pub enum ExtensionType {
    #[strum(disabled)]
    Handshake,
    #[strum(to_string = "ut_metadata")]
    Metadata,
}

// the handshake is always active
pub const ACTIVE_EXTENSIONS: &[ExtensionType] = &[ExtensionType::Metadata];
