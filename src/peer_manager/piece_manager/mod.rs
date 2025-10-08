use std::{
    fs::{File, OpenOptions},
    path::PathBuf,
};

use crate::{
    Torrent,
    database::DBConnection,
    peer_manager::{MAX_PIECES_IN_PARALLEL, PieceState, error::PeerManagerError},
};
mod file_manager;
mod req_preparer;

pub(super) struct PieceManager {
    /// I need this information too often to always query the DB
    /// so let's cache it
    pub(crate) have: Vec<bool>,
    /// if it's None, we are finished
    pub(crate) download_queue: Vec<PieceState>,
    pub(crate) db_conn: DBConnection,
    /// the output file
    pub(crate) file: File,
}

impl PieceManager {
    pub(super) async fn new(
        db_conn: DBConnection,
        file_path: Option<PathBuf>,
        torrent: &Torrent,
    ) -> Result<Self, PeerManagerError> {
        let file_path = file_path.unwrap_or(torrent.info.name.clone().into());
        let file_entry = db_conn.get_entry().await?;
        let file_existed = file_entry.is_some();
        let file_entry = if let Some(file_entry) = file_entry {
            file_entry
        } else {
            db_conn.set_entry(file_path, torrent.clone()).await?
        };

        let file = OpenOptions::new()
            .create(!file_existed)
            .append(true)
            .truncate(false)
            .open(&file_entry.file)
            .map_err(|error| PeerManagerError::OpenError {
                path: file_entry.file.to_path_buf(),
                error,
            })?;

        let download_queue = if file_entry.is_finished() {
            todo!("We are finished and now seeding which isn't implemented yet.")
        } else {
            Vec::with_capacity(MAX_PIECES_IN_PARALLEL)
        };

        Ok(PieceManager {
            have: file_entry.bitfield.to_vec(),
            download_queue,
            db_conn,
            file,
        })
    }
}
