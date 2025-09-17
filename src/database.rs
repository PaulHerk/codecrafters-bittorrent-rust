use std::borrow::Cow;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use surrealdb::RecordId;
use surrealdb::Surreal;
use surrealdb::engine::local::Db;
use surrealdb::opt::PatchOp;

// For a RocksDB file
use surrealdb::engine::local::RocksDb;

use crate::Torrent;

/// the actual data stored in the DB
/// torrent path is also the key
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FileInfo {
    pub bitfield: Cow<'static, [bool]>,
    pub file: Cow<'static, Path>,
    // TODO: don't store path, rather the content
    pub torrent: Cow<'static, Path>,
}

impl FileInfo {
    fn from_new_file(file_path: PathBuf, torrent_path: PathBuf, piece_length: usize) -> Self {
        Self {
            bitfield: (vec![false; piece_length]).into(),
            file: file_path.into(),
            torrent: torrent_path.into(),
        }
    }

    pub fn is_finished(&self) -> bool {
        self.bitfield.iter().all(|b| *b)
    }
}

#[derive(Debug, Deserialize)]
struct Record {
    #[allow(dead_code)]
    id: RecordId,
}
#[derive(Debug, Clone)]
pub struct DBConnection(pub Surreal<Db>);

impl DBConnection {
    pub async fn new() -> anyhow::Result<Self> {
        let db = Surreal::new::<RocksDb>("files").await?;
        db.use_ns("files_ns").use_db("files_db").await?;
        Ok(Self(db))
    }

    pub async fn set_and_get_file(
        &self,
        file_path: Option<PathBuf>,
        torrent_path: PathBuf,
        torrent: &Torrent,
    ) -> anyhow::Result<FileInfo> {
        // read the torrent file
        let info_hash_hex = &hex::encode(torrent.info_hash());

        let file_info = match self
            .0
            .create::<Option<FileInfo>>(("files", info_hash_hex))
            .await
        {
            Ok(_created) => {
                // get the file path, create a new entry and insert it
                let file_path = match file_path {
                    Some(path) => path,
                    None => PathBuf::from(&torrent.info.name),
                };
                let file = FileInfo::from_new_file(
                    file_path,
                    torrent_path,
                    torrent.info.piece_length as usize,
                );
                let file_info: Option<FileInfo> = self
                    .0
                    .insert(("files", info_hash_hex))
                    .content(file)
                    .await?;
                return Ok(file_info.expect("Not even sure when this is None in the first place.."));
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
    ) -> anyhow::Result<()> {
        let updated: Option<FileInfo> = self
            .0
            .update(("files", info_hash))
            .patch(PatchOp::replace("/bitfield", new_bitfield))
            .await
            .context("Updating the DB entry.")?;
        dbg!(&updated);

        assert!(
            updated.is_some(),
            "The record for the torrent was already created if wasn't there."
        );

        Ok(())
    }
}
