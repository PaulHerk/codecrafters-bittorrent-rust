use std::fs::File;

use crate::{database::DBConnection, peer_manager::PieceState};
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
