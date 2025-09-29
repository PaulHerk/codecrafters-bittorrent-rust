use std::{io, path::PathBuf};

use thiserror::Error;
use tokio::sync::mpsc::error::SendError;

use crate::{database::DBError, peer_manager::ResMessage, torrent::TorrentError};
#[derive(Debug, Error)]
pub enum PeerManagerError {
    #[error("The request-manager failed with the following DB error: {0}")]
    DB(#[from] DBError),
    #[error("The request-manager failed with the following Torrent error: {0}")]
    Torrent(#[from] TorrentError),
    #[error("Failed to open the file at the path `{path}` with the error: `{error}`")]
    OpenError { path: PathBuf, error: io::Error },
    #[error(
        "Failed to send a message: `{msg}` to peer with ID {peer_id:?} with the error: `{error}`"
    )]
    SendError {
        peer_id: [u8; 20],
        error: SendError<ResMessage>,
        msg: String,
    },
    /// This error should never happen realistically. It would happen if the RequestManager needs to
    /// get a peer state from a peer that hasn't sent the NewConnection message yet. This message is
    /// sent by every peer at the beginning though.
    #[error(
        "An internal error occured: the peer that was requested was not found in the current list."
    )]
    PeerNotFound,
    #[error("An error occured when writing to the file: `{0}`")]
    WritingToFile(#[from] io::Error),
}
