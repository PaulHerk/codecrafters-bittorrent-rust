use std::borrow::Cow;
use std::fs::File;
use std::fs::OpenOptions;
use std::io;
use std::io::Read;
use std::os::unix::fs::FileExt;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;

use anyhow::Context;
use bytes::BytesMut;
use itertools::Itertools;
use rand::rng;
use rand::seq::IteratorRandom;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use surrealdb::RecordId;
use surrealdb::Surreal;
use surrealdb::engine::local::Db;

// For a RocksDB file
use surrealdb::engine::local::RocksDb;
use tokio::sync::mpsc::Receiver;

use crate::BLOCK_MAX;
use crate::RequestPiecePayload;
use crate::ResponsePiecePayload;
use crate::Torrent;

/// Information about a file that gets downloaded/seeded
#[derive(Debug, Clone)]
pub struct DBFile {
    pub file_path: PathBuf,
    pub torrent_path: PathBuf,
}

/// the actual data stored in the DB
/// torrent path is also the key
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FileInfo {
    bitfield: Cow<'static, [bool]>,
    file: Cow<'static, Path>,
    torrent: Cow<'static, Path>,
}

#[derive(Debug, Serialize)]
struct BitfieldUpdate {
    bitfield: Cow<'static, [bool]>,
}

impl FileInfo {
    fn from_file(file: DBFile) -> Self {
        Self {
            bitfield: (&[]).into(),
            file: file.file_path.into(),
            torrent: file.torrent_path.into(),
        }
    }
}

#[derive(Debug, Deserialize)]
struct Record {
    #[allow(dead_code)]
    id: RecordId,
}
#[derive(Debug, Clone)]
pub struct DBConnection(pub Surreal<Db>);

/// used for down- and uploading data from a file
#[derive(Debug, Clone)]
pub struct FileLoader {
    file_info: Arc<Mutex<FileInfo>>,
    pub torrent: Arc<Torrent>,
    file: Arc<Mutex<File>>,
    /// the info_hash is hex encoded here
    info_hash: [char; 40],
    /// None if we're finished downloading
    current_download_piece: Arc<Mutex<Option<u32>>>,
    current_download_block: Arc<Mutex<Option<u32>>>,
    // TODO: db_conn: Arc<Surreal<Db>> ??
    db_conn: Arc<Surreal<Db>>,
}

fn read_torrent(torrent: &PathBuf) -> anyhow::Result<Torrent> {
    let bytes = std::fs::read(torrent).context("read torrent file")?;
    let torrent = serde_bencode::from_bytes::<Torrent>(&bytes).context("decode torrent")?;

    Ok(torrent)
}

impl FileLoader {
    pub async fn from_db_file(db_file: DBFile) -> anyhow::Result<Self> {
        let data_file = OpenOptions::new()
            .create(true)
            .write(true)
            .open(&db_file.file_path)
            .context("opening data file")?;
        let torrent = read_torrent(&db_file.torrent_path)?;
        let info_hash_hex = hex::encode(torrent.info_hash()?);

        let db_conn = DBConnection::new().await?;
        let info_hash_u10: [char; 40] = info_hash_hex
            .chars()
            .collect::<Vec<char>>()
            .try_into()
            .expect("The SHA1 hash has by definition 40 hex chars");
        let file_info = db_conn
            .set_and_get_file(db_file, &info_hash_hex)
            .await
            .context("context")?;

        let current_download_piece = match file_info.bitfield.iter().all(|b| *b) {
            true => None,
            false => Some(rand::random_range(0..torrent.info.pieces.0.len()) as u32),
        };
        let current_download_block = match current_download_piece.is_some() {
            true => Some(0),
            false => None,
        };

        Ok(Self {
            file_info: Arc::new(Mutex::new(file_info)),
            torrent: Arc::new(torrent),
            file: Arc::new(Mutex::new(data_file)),
            info_hash: info_hash_u10,
            current_download_piece: Arc::new(Mutex::new(current_download_piece)),
            current_download_block: Arc::new(Mutex::new(current_download_block)),
            db_conn: Arc::new(db_conn.0),
        })
    }
    async fn get_piece(&self, piece_i: u32) -> Result<(), io::Error> {
        let piece_len = self.torrent.info.piece_length;
        let file = &mut self.file.lock().unwrap();
        let mut piece_buf = BytesMut::with_capacity(piece_len as usize);
        file.read_exact_at(&mut piece_buf, piece_i as u64 * piece_len as u64)
    }
    pub fn get_block(&self, req_payload: RequestPiecePayload) -> Option<Vec<u8>> {
        todo!()
    }

    fn is_finished(&self) -> bool {
        self.file_info.lock().unwrap().bitfield.iter().all(|b| *b)
    }

    pub async fn write_file(
        &self,
        mut data_rx: Receiver<ResponsePiecePayload>,
    ) -> anyhow::Result<()> {
        let piece_length = self.torrent.info.piece_length as usize;
        let info_hash: String = self.info_hash.iter().collect();

        let mut pieces_recv: u32 = 0;
        let mut piece = BytesMut::with_capacity(piece_length);
        let mut current_piece_i: u32 = 0;

        while let Some(block) = data_rx.recv().await {
            // first, set the current_piece after each piece finished writing
            if piece.len() == 0 {
                current_piece_i = block.index;
            }
            // ignore if wrong piece_index
            if current_piece_i != block.index {
                eprintln!("Got wrong piece!");
                continue;
            }

            piece.extend_from_slice(&block.block);

            // if piece is complete, check hash, write it to file & save bitfield to DB
            if piece.len() == piece_length {
                let mut sha1 = Sha1::new();
                sha1.update(&piece);
                let hash: [u8; 20] = sha1.finalize().try_into().expect("SHA1 is always 20 bytes");
                let torrent_hash = self.torrent.info.pieces.0[current_piece_i as usize];
                if hash != torrent_hash {
                    piece.clear();
                    continue;
                }

                self.file
                    .lock()
                    .unwrap()
                    .write_all_at(&piece, current_piece_i as u64 * piece_length as u64)
                    .context("write piece")?;
                let mut new_bitfield = self.file_info.lock().unwrap().bitfield.to_vec();
                new_bitfield[current_piece_i as usize] = true;

                let updated: Option<Record> = self
                    .db_conn
                    .update(("files", &info_hash))
                    .merge(BitfieldUpdate {
                        bitfield: new_bitfield.into(),
                    })
                    .await?;

                if let None = updated {
                    panic!("what")
                }

                piece.clear();

                println!("piece {} complete", current_piece_i);

                pieces_recv += 1;
                if pieces_recv == self.torrent.info.pieces.0.len() as u32 {
                    break;
                }
            }
        }

        assert_eq!(
            self.file
                .lock()
                .unwrap()
                .metadata()
                .context("get file metadata")?
                .len(),
            self.torrent.get_length() as u64
        );
        println!("File download complete.");
        Ok(())
    }

    /// returns the next piece and block begin that we need to request
    /// if we have all the blocks of a piece, we return None
    pub(super) fn prepare_next_req_send(&self, peer_has: &[bool]) -> Option<RequestPiecePayload> {
        if self.is_finished() {
            return None;
        }
        if let Some(piece_i) = *self.current_download_piece.lock().unwrap() {
            let i_have = &self.file_info.lock().unwrap().bitfield;

            // if I already have the piece or the block is none (which should not be the case however), choose the next one
            let cur_block = &mut *self.current_download_block.lock().unwrap();
            if i_have[piece_i as usize] || cur_block.is_none() {
                let new_piece_i = peer_has
                    .iter()
                    .enumerate()
                    .zip(i_have.iter().enumerate())
                    .filter_map(|((i_peer, b_p), (i_self, b_s))| {
                        if !*b_s && *b_p {
                            // essentially I put both the bitfields toghether (which are in equal length)
                            assert_eq!(i_peer, i_self);
                            Some(i_peer as u32)
                        } else {
                            None
                        }
                    })
                    .choose(&mut rng());

                *self.current_download_piece.lock().unwrap() = new_piece_i;
                *cur_block = if new_piece_i.is_some() { Some(0) } else { None };
                return self.prepare_next_req_send(peer_has);
            }

            //
            // choose next block
            //
            let cur_block_unwrapped =
                cur_block.expect("we just checked whether it's none and updated it if it is");

            // TODO
            let (piece_size, nblocks) = self.get_piece_size_n_blocks(piece_i);
            *cur_block = if cur_block_unwrapped == nblocks - 2 {
                Some(cur_block_unwrapped + 1)
            } else {
                None
            };

            let Some(block_i) = *cur_block else {
                return self.prepare_next_req_send(peer_has);
            };

            let block_length = self.get_block_len((piece_size, nblocks), block_i);

            let req = RequestPiecePayload::new(piece_i as u32, block_i * BLOCK_MAX, block_length);
            Some(req)
        } else {
            None
        }
    }

    /// returns piece_size and n_blocks
    fn get_piece_size_n_blocks(&self, piece_i: u32) -> (u32, u32) {
        let torrent = &self.torrent;
        let length = torrent.get_length();
        let piece_size = if piece_i == torrent.info.pieces.0.len() as u32 - 1
            && length % torrent.info.piece_length != 0
        {
            length % torrent.info.piece_length
        } else {
            torrent.info.piece_length
        };
        // the "+ BLOCK_MAX - 1" rounds up
        let nblocks = (piece_size + BLOCK_MAX - 1) / BLOCK_MAX;
        (piece_size, nblocks)
    }
    fn get_block_len(&self, (piece_size, nblocks): (u32, u32), block_i: u32) -> u32 {
        let block_length = if block_i == nblocks - 1 && piece_size % BLOCK_MAX != 0 {
            piece_size % BLOCK_MAX
        } else {
            BLOCK_MAX
        };

        block_length
    }
}

impl DBConnection {
    pub async fn new() -> anyhow::Result<Self> {
        let db = Surreal::new::<RocksDb>("files").await?;
        db.use_ns("files_ns").use_db("files_db").await?;
        Ok(Self(db))
    }

    pub async fn set_and_get_file(
        &self,
        file: DBFile,
        info_hash_hex: &str,
    ) -> surrealdb::Result<FileInfo> {
        // Create a file entry
        let file = FileInfo::from_file(file);
        let file_info = match self
            .0
            .create(("files", hex::encode(info_hash_hex)))
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

        Ok(file_info)
    }
}
