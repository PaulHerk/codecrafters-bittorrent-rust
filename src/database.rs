use std::borrow::Cow;
use std::path::Path;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use surrealdb::RecordId;
use surrealdb::Surreal;
use surrealdb::engine::local::Db;
use surrealdb::opt::PatchOp;

// For a RocksDB file
use surrealdb::engine::local::RocksDb;
use thiserror::Error;

use crate::torrent::{InfoHash, Metainfo, Torrent};

/// the actual data stored in the DB
/// torrent path is also the key
#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct DBEntry {
    pub(crate) bitfield: Cow<'static, [bool]>,
    pub(crate) file: Cow<'static, Path>,
    pub(crate) torrent_info: Metainfo,
    pub(crate) announce: url::Url,
}

impl DBEntry {
    fn from_new_file(file_path: PathBuf, torrent: Torrent) -> Self {
        let n_pieces = torrent.info.pieces.0.len();
        Self {
            bitfield: (vec![false; n_pieces]).into(),
            file: file_path.into(),
            torrent_info: torrent.info,
            announce: torrent.announce,
        }
    }

    pub(crate) fn is_finished(&self) -> bool {
        self.bitfield.iter().all(|b| *b)
    }
}

#[derive(Debug, Deserialize)]
struct Record {
    #[allow(dead_code)]
    id: RecordId,
}
#[derive(Debug, Clone)]
pub(crate) struct DBConnection {
    pub(crate) db: Surreal<Db>,
    pub(crate) info_hash_hex: String,
}

impl DBConnection {
    pub(crate) async fn new(info_hash: InfoHash) -> Result<DBConnection, DBError> {
        let info_hash_hex = hex::encode(info_hash.0);
        let db = Surreal::new::<RocksDb>("files").await?;
        db.use_ns("files_ns").use_db("files_db").await?;
        Ok(Self { db, info_hash_hex })
    }

    pub(crate) async fn get_entry(&self) -> Result<Option<DBEntry>, DBError> {
        let entry = self.db.select(("files", &self.info_hash_hex)).await?;
        Ok(entry)
    }

    pub(crate) async fn set_entry(
        &self,
        file_path: PathBuf,
        torrent: Torrent,
    ) -> Result<DBEntry, DBError> {
        let file = DBEntry::from_new_file(file_path, torrent);
        let entry = self
            .db
            .create::<Option<DBEntry>>(("files", &self.info_hash_hex))
            .content(file)
            .await?
            .expect("I'm really curious what the error is here.");
        Ok(entry)
    }

    pub(super) async fn update_bitfields(
        &mut self,
        new_bitfield: Vec<bool>,
    ) -> Result<(), DBError> {
        let updated: Option<DBEntry> = self
            .db
            .update(("files", &self.info_hash_hex))
            .patch(PatchOp::replace("/bitfield", new_bitfield))
            .await?;

        assert!(
            updated.is_some(),
            "The record for the torrent was already created if wasn't there."
        );

        Ok(())
    }
}

#[derive(Error, Debug)]
pub enum DBError {
    #[error("Got error from the local DB: `{0}`")]
    DBError(Box<surrealdb::Error>),
}

impl From<surrealdb::Error> for DBError {
    fn from(value: surrealdb::Error) -> Self {
        Self::DBError(Box::new(value))
    }
}
