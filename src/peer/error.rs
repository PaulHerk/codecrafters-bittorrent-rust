use std::{io, mem::Discriminant, net::SocketAddrV4};

use thiserror::Error;
use tokio::sync::mpsc;

use crate::peer_manager::{ReqMessage, ReqMsgFromPeer};

#[derive(Error, Debug)]
pub enum PeerError {
    #[error(
        "Failed to send a message with type {msg_type_str} to a remote peer with the id: `{peer_id:?}` with the error: `{error}`."
    )]
    SendToPeer {
        error: io::Error,
        peer_id: [u8; 20],
        msg_type_str: String,
    },
    #[error(
        "Failed to send a message with type `{msg_type:?}` from the peer with id `{peer_id:?}` to the PeerManager with error: `{error}`"
    )]
    SendToPeerManager {
        error: mpsc::error::SendError<ReqMsgFromPeer>,
        peer_id: [u8; 20],
        msg_type: Discriminant<ReqMessage>,
    },
    #[error("The peer unexpectedly disconnected.")]
    PeerDisconnected,
    #[error("Failed to establish a tcp connection to the address `{addr}` with error: `{error:?}`")]
    FailedToConnect {
        error: io::Error,
        addr: SocketAddrV4,
    },
    #[error(
        "Failed to read the bytes from the remote peer needed for the handshake with the error: `{0}`."
    )]
    RecvHandshake(io::Error),
    #[error("Failed to decode the handshake received from the peer with the error: `{0}`")]
    DecodeHandshake(#[from] bincode::error::DecodeError),
    #[error("Failed to decode the handshake received from the peer with the error: `{0}`")]
    EncodeHandshake(#[from] bincode::error::EncodeError),
}
