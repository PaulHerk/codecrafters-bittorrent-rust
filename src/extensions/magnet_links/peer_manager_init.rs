use std::{collections::HashMap, fs::OpenOptions, path::PathBuf};

use tokio::sync::mpsc;

use crate::{
    PeerManager,
    database::{DBConnection, DBEntry},
    magnet_links::{MagnetLink, MagnetLinkError, before_download_manager::BeforePeerManager},
    peer_manager::{MAX_PIECES_IN_PARALLEL, ReqMsgFromPeer, error::PeerManagerError},
};

impl PeerManager {
    pub async fn init_from_magnet(
        rx: mpsc::Receiver<ReqMsgFromPeer>,
        file_path: Option<PathBuf>,
        magnet_link: &str,
        peer_id: &[u8; 20],
        port: u16,
    ) -> Result<Self, MagnetLinkError> {
        let magnet = MagnetLink::from_url(magnet_link)?;
        let db_conn = DBConnection::new().await?;
        let info_hash_hex = hex::encode(magnet.info_hash.0);
        if let Some(entry) = db_conn.get_entry(&info_hash_hex).await? {
            PeerManager::from_entry(entry, db_conn, rx, info_hash_hex)
        } else {
            let before_download_manager =
                BeforePeerManager::from_magnet(magnet, rx, peer_id, port).await?;
            before_download_manager.to_peer_manager(file_path)
        }
    }

    fn from_entry(
        entry: DBEntry,
        db_conn: DBConnection,
        rx: mpsc::Receiver<ReqMsgFromPeer>,
        info_hash_hex: String,
    ) -> Result<Self, MagnetLinkError> {
        let download_queue = if entry.is_finished() {
            None
        } else {
            Some(Vec::with_capacity(MAX_PIECES_IN_PARALLEL))
        };
        let file = OpenOptions::new()
            .create(false)
            .append(true)
            .truncate(false)
            .open(&entry.file)
            .map_err(|error| PeerManagerError::OpenError {
                path: entry.file.to_path_buf(),
                error,
            })?;
        Ok(PeerManager {
            file,
            db_conn,
            rx,
            have: entry.bitfield.to_vec(),
            download_queue,
            torrent_info: entry.torrent_info,
            announce_url: entry.announce,
            info_hash_hex,
            peers: HashMap::new(),
        })
    }
}
