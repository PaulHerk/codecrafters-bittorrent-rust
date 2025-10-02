use std::fs::OpenOptions;
use std::{collections::HashMap, path::PathBuf};
use tokio::sync::mpsc;

use crate::TrackerRequest;
use crate::{
    PeerManager,
    database::DBConnection,
    magnet_links::{MagnetLink, MagnetLinkError},
    peer_manager::{PeerConn, ReqMsgFromPeer, error::PeerManagerError},
    torrent::File,
};

#[derive(Debug)]
pub(crate) struct BeforePeerManager {
    /// the output file
    pub(super) rx: mpsc::Receiver<ReqMsgFromPeer>,
    pub announce_url: url::Url,
    pub info_hash_hex: String,
    magnet: MagnetLink,
    pub(super) peers: HashMap<[u8; 20], PeerConn>,
}

impl BeforePeerManager {
    pub(super) async fn from_magnet(
        magnet: MagnetLink,
        rx: mpsc::Receiver<ReqMsgFromPeer>,
        peer_id: &[u8; 20],
        port: u16,
    ) -> Result<Self, MagnetLinkError> {
        // using 999 as a placeholder since we don't know the length yet
        let req = TrackerRequest::new(&magnet.info_hash, peer_id, port, 999);
        let Some(announce_url) = magnet.announce else {
            return Err(MagnetLinkError::NoTrackerUrl);
        };
        let res = req.get_response(announce_url).await?;

        todo!()
    }
    pub(super) fn to_peer_manager(
        self,
        file_path: Option<PathBuf>,
    ) -> Result<PeerManager, MagnetLinkError> {
        let file_path = file_path
            .or(self.magnet.file_name.map(|n| n.into()))
            .ok_or(PeerManagerError::NoFileName)?;
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .truncate(false)
            .open(&file_path)
            .map_err(|error| PeerManagerError::OpenError {
                path: file_path,
                error,
            })?;
        todo!()
    }
}
