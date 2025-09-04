use std::borrow::Cow;
use std::path::Path;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use surrealdb::RecordId;
use surrealdb::Surreal;
use surrealdb::engine::local::Db;

// For a RocksDB file
use surrealdb::engine::local::RocksDb;

/// Information about a file that gets downloaded/seeded
pub struct DBFile {
    pub file_path: PathBuf,
    pub torrent_path: PathBuf,
    pub info_hash: [u8; 20],
}

/// the actual data stored in the DB
/// torrent path is also the key
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FileInfo {
    bitfield: Cow<'static, [bool]>,
    file: Cow<'static, Path>,
    torrent: Cow<'static, Path>,
    info_hash: [u8; 20],
}

impl FileInfo {
    fn from_file(file: DBFile) -> Self {
        Self {
            bitfield: (&[]).into(),
            file: file.file_path.into(),
            torrent: file.torrent_path.into(),
            info_hash: file.info_hash,
        }
    }
}

#[derive(Debug, Deserialize)]
struct Record {
    #[allow(dead_code)]
    id: RecordId,
}

pub struct DBConnection(Surreal<Db>);

impl DBConnection {
    pub async fn new() -> anyhow::Result<Self> {
        let db = Surreal::new::<RocksDb>("files").await?;
        db.use_ns("files_ns").use_db("files_db").await?;
        Ok(Self(db))
    }

    pub async fn set_and_get_file(&self, file: DBFile) -> surrealdb::Result<FileInfo> {
        // Create a file entry
        let file = FileInfo::from_file(file);
        let file_info = match self
            .0
            .create(("files", hex::encode(file.info_hash)))
            .content(file.clone())
            .await
        {
            Ok(created) => Ok(created.expect("How can this be None?")),
            Err(e) => {
                if let surrealdb::Error::Db(ref e) = e
                    && let surrealdb::error::Db::RecordExists { thing } = e
                {
                    dbg!(&thing);
                    let record: Option<FileInfo> = self
                        .0
                        .select((thing.tb.as_str(), thing.id.to_raw()))
                        .await?;
                    dbg!(&record);
                    Ok(record.expect("It must exist since we just checked that it exists"))
                } else {
                    Err(e)
                }
            }
        }?;
        dbg!(&file_info);

        // // Update a person record with a specific id
        // let updated: Option<Record> = db
        //     .update(("person", "jaime"))
        //     .merge(Responsibility { marketing: true })
        //     .await?;
        // dbg!(updated);

        // // Select all people records
        // let people: Vec<FileInfo> = db.select("files").await?;
        // dbg!(people);

        // // Perform a custom advanced query
        // let groups = db
        //     .query("SELECT marketing, count() FROM type::table($table) GROUP BY marketing")
        //     .bind(("table", "person"))
        //     .await?;
        // dbg!(groups);

        Ok(file_info)
    }
}
