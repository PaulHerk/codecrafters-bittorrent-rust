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

/// the actual data stored in the DB
/// torrent path is also the key
#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct FileInfo {
    pub(crate) bitfield: Cow<'static, [bool]>,
    pub(crate) file: Cow<'static, Path>,
    // TODO: don't store path, rather the content
    pub(crate) torrent: Cow<'static, Path>,
}

impl FileInfo {
    fn from_new_file(file_path: PathBuf, torrent_path: PathBuf, n_pieces: usize) -> Self {
        Self {
            bitfield: (vec![false; n_pieces]).into(),
            file: file_path.into(),
            torrent: torrent_path.into(),
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
        file_path: Option<PathBuf>,
        torrent_path: PathBuf,
        torrent: &Torrent,
    ) -> Result<FileInfo, DBError> {
        // read the torrent file
        let info_hash_hex = &hex::encode(torrent.info_hash());
        // get the file path, create a new entry and insert it
        let file_path = match file_path {
            Some(path) => path,
            None => PathBuf::from(&torrent.info.name),
        };
        let file = FileInfo::from_new_file(file_path, torrent_path, torrent.info.pieces.0.len());

        // so I have to create the content already since it would complain that the content is invalid
        // TODO: do with select
        let file_info = match self
            .0
            .create::<Option<FileInfo>>(("files", info_hash_hex))
            .content(file)
            .await
        {
            Ok(created) => {
                return Ok(created.expect("Not even sure when this is None in the first place.."));
            }
            Err(e) => {
                if let surrealdb::Error::Db(ref e) = e
                    && let surrealdb::error::Db::RecordExists { thing } = e
                {
                    let record: Option<FileInfo> = self
                        .0
                        .select((thing.tb.as_str(), thing.id.to_raw()))
                        .await?;
                    Ok(record.expect("It must exist since we just checked that it exists"))
                } else {
                    Err(e)
                }
            }
        }?;

        Ok(file_info)
    }

    pub(super) async fn update_bitfields(
        &mut self,
        info_hash: &str,
        new_bitfield: Vec<bool>,
    ) -> Result<(), DBError> {
        let updated: Option<FileInfo> = self
            .0
            .update(("files", info_hash))
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
pub(crate) enum DBError {
    #[error("Got error from the local DB: `{0}`")]
    DBError(#[from] surrealdb::Error),
}
