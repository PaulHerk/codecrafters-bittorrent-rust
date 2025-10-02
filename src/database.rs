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

use crate::Torrent;
use crate::torrent::Info;
use crate::torrent::InfoHash;

/// the actual data stored in the DB
/// torrent path is also the key
#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct DBEntry {
    pub(crate) bitfield: Cow<'static, [bool]>,
    pub(crate) file: Cow<'static, Path>,
    // TODO: don't store path, rather the content
    pub(crate) torrent_info: Info,
    pub(crate) announce: url::Url,
}

impl DBEntry {
    fn from_new_file(file_path: PathBuf, torrent_info: Info, announce: url::Url) -> Self {
        let n_pieces = torrent_info.pieces.0.len();
        Self {
            bitfield: (vec![false; n_pieces]).into(),
            file: file_path.into(),
            torrent_info,
            announce,
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
pub(crate) struct DBConnection(pub(crate) Surreal<Db>);

impl DBConnection {
    pub(crate) async fn new() -> Result<DBConnection, DBError> {
        let db = Surreal::new::<RocksDb>("files").await?;
        db.use_ns("files_ns").use_db("files_db").await?;
        Ok(Self(db))
    }

    pub(crate) async fn set_and_get_file(
        &self,
        file_path: PathBuf,
        info_hash: &InfoHash,
        torrent_info: Info,
        announce: url::Url,
    ) -> Result<DBEntry, DBError> {
        let info_hash_hex = &hex::encode(info_hash.0);

        if let Some(entry) = self.get_entry(info_hash_hex).await? {
            Ok(entry)
        } else {
            let file = DBEntry::from_new_file(file_path, torrent_info, announce);
            let entry = self
                .0
                .create::<Option<DBEntry>>(("files", info_hash_hex))
                .content(file)
                .await?
                .expect("I'm really curious what the error is here.");
            Ok(entry)
        }
    }

    pub(crate) async fn get_entry(&self, info_hash_hex: &str) -> Result<Option<DBEntry>, DBError> {
        let entry = self.0.select(("files", info_hash_hex)).await?;
        Ok(entry)
    }

    pub(super) async fn update_bitfields(
        &mut self,
        info_hash_hex: &str,
        new_bitfield: Vec<bool>,
    ) -> Result<(), DBError> {
        let updated: Option<DBEntry> = self
            .0
            .update(("files", info_hash_hex))
            .patch(PatchOp::replace("/bitfield", new_bitfield))
            .await?;
        dbg!(&updated);

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
