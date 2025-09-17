use std::borrow::Cow;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use surrealdb::RecordId;
use surrealdb::Surreal;
use surrealdb::engine::local::Db;
use surrealdb::opt::PatchOp;

// For a RocksDB file
use surrealdb::engine::local::RocksDb;
use tracing_mutex::stdsync::Mutex;

use crate::BLOCK_MAX;
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

/// used for up- and downloading data from a file
// #[derive(Debug, Clone)]
// pub struct FileLoader {
//     file_info: Arc<Mutex<FileInfo>>,
//     pub torrent: Arc<Torrent>,
//     file: Arc<Mutex<File>>,
//     /// the info_hash is hex encoded here
//     info_hash: [char; 40],
//     /// None if we're finished downloading, TODO: I might not even need the Option?
//     /// TODO: I need to keep track of the next download-piece aswell
//     download_state: Arc<Mutex<VecDeque<DownloadState>>>,
//     // TODO: db_conn: Arc<Surreal<Db>> ??
//     db_conn: Arc<Surreal<Db>>,
// }

#[derive(Debug, Clone)]
struct DownloadState {
    piece_i: u32,
    block_i: u32,
    piece: Arc<Mutex<Vec<u8>>>,
}
impl DownloadState {
    fn get_n_blocks(&self) -> u32 {
        (self.piece.lock().unwrap().capacity() as u32).div_ceil(BLOCK_MAX)
    }

    fn get_block_len(&self) -> u32 {
        let piece_size = self.piece.lock().unwrap().capacity() as u32;

        if self.block_i == self.get_n_blocks() - 1 && piece_size % BLOCK_MAX != 0 {
            piece_size % BLOCK_MAX
        } else {
            BLOCK_MAX
        }
    }
}

// impl FileLoader {
//     pub async fn from_db_file(db_file: DBFile) -> anyhow::Result<Self> {
//         let data_file = OpenOptions::new()
//             .create(true)
//             .write(true)
//             .truncate(false)
//             .open(&db_file.file_path)
//             .context("opening data file")?;
//         let torrent = read_torrent(&db_file.torrent_path)?;
//         let info_hash_hex = hex::encode(torrent.info_hash()?);

//         let db_conn = DBConnection::new().await?;
//         let info_hash_u10: [char; 40] = info_hash_hex
//             .chars()
//             .collect::<Vec<char>>()
//             .try_into()
//             .expect("The SHA1 hash has by definition 40 hex chars");
//         let file_info = db_conn
//             .set_and_get_file(db_file, &info_hash_hex, torrent.info.pieces.0.len())
//             .await
//             .context("context")?;
//         dbg!(&file_info);

//         let mut download_state = VecDeque::with_capacity(MAX_PIECES_IN_PARALLEL);
//         let piece_i = rand::random_range(0..torrent.info.pieces.0.len() - 1) as u32; // -1 because we don't want to download the last piece since it might not be piece_length in size
//         download_state.push_back(DownloadState {
//             piece_i,
//             block_i: 0,
//             piece: Arc::new(Mutex::new(Vec::with_capacity(
//                 torrent.info.piece_length as usize,
//             ))),
//         });

//         Ok(Self {
//             file_info: Arc::new(Mutex::new(file_info)),
//             torrent: Arc::new(torrent),
//             file: Arc::new(Mutex::new(data_file)),
//             info_hash: info_hash_u10,
//             download_state: Arc::new(Mutex::new(download_state)),
//             db_conn: Arc::new(db_conn.0),
//         })
//     }

//     pub fn is_finished(&self) -> bool {
//         self.download_state.lock().unwrap().is_empty()
//             || self.file_info.lock().unwrap().is_finished()
//     }

//     pub(super) fn get_block(&self, req_payload: RequestPiecePayload) -> Option<Vec<u8>> {
//         let mut buf = BytesMut::with_capacity(req_payload.length as usize);
//         let offset = req_payload.index as u64 * self.torrent.info.piece_length as u64
//             + req_payload.begin as u64;
//         self.file
//             .lock()
//             .unwrap()
//             .read_exact_at(&mut buf, offset)
//             .ok()?;
//         Some(buf.to_vec())
//     }

//     /// returns the next piece and block begin that we need to request
//     /// if we have all the blocks of a piece, we return None
//     /// also it writes the piece to the file if the piece is complete
//     pub(super) fn prepare_next_req_send(&self, peer_has: &[bool]) -> Option<RequestPiecePayload> {
//         let state = &mut *self.download_state.lock().unwrap();
//         if state.is_empty() {
//             return None;
//         }
//         let i_have = &self.file_info.lock().unwrap().bitfield;

//         // if I already have the piece, append a new one to the back of the queue
//         // this can happen if we have multiple threads
//         if let Some(front) = state.front()
//             && i_have[front.piece_i as usize]
//             && state.len() < MAX_PIECES_IN_PARALLEL
//         {
//             state.push_back(self.get_next_download_state(peer_has, i_have)?);
//         }
//         if state.len() == MAX_PIECES_IN_PARALLEL {
//             todo!("help other thread which might have a slow upload rate");
//         }

//         //
//         // choose next block
//         //
//         let back = state
//             .back_mut()
//             .expect("we checked that the queue is not empty");
//         let nblocks = back.get_n_blocks();
//         let block_length = back.get_block_len();
//         let req = RequestPiecePayload::new(back.piece_i, back.block_i * BLOCK_MAX, block_length);

//         // increment download_state if it's not the last one, otherwise calculate the next piece again
//         // note: block_i starts at 0 and nblocks is a len but block_i is the index
//         // of the block we want to write next and not the index of the block we return to write to
//         if back.block_i == nblocks {
//             if state.len() < MAX_PIECES_IN_PARALLEL {
//                 state.push_back(self.get_next_download_state(peer_has, i_have)?);
//             } else {
//                 todo!("help other thread which might have a slow upload rate");
//             }
//         } else {
//             back.block_i += 1;
//         }
//         dbg!(&req);
//         Some(req)
//     }

//     // this function writes the piece to the file and updates the bitfield in Self and the DB
//     pub(super) async fn write_piece(&self, payload: ResponsePiecePayload) -> anyhow::Result<()> {
//         let new_bitfield: Cow<'static, [bool]> = {
//             let state = &mut *self.download_state.lock().unwrap();
//             let Some(cur_state) = state.iter_mut().find(|s| s.piece_i == payload.index) else {
//                 return Ok(());
//             };
//             let nblocks = cur_state.get_n_blocks();
//             dbg!(&nblocks, &cur_state);

//             // the block_i is the index of the block we want to write
//             // TODO: this is a huge issue since if we have multiple threads, one might already be at the next piece before we get here
//             if payload.begin != (cur_state.block_i - 1) * BLOCK_MAX {
//                 return Err(anyhow::anyhow!(
//                     "payload is not the next block we want to write"
//                 ));
//             }
//             // assert_eq!(payload.indexstate.piece_i);
//             // assert_eq!(payload.begin, (state.block_i - 1) * BLOCK_MAX);
//             cur_state
//                 .piece
//                 .lock()
//                 .unwrap()
//                 .extend_from_slice(&payload.block);

//             if cur_state.block_i < nblocks {
//                 // we can't write the piece yet
//                 return Ok(());
//             }

//             let mut sha1 = Sha1::new();
//             sha1.update(&cur_state.piece.lock().unwrap().as_slice());
//             let hash: [u8; 20] = sha1.finalize().into();
//             let torrent_hash = self.torrent.info.pieces.0[cur_state.piece_i as usize];
//             if hash != torrent_hash {
//                 return Err(anyhow::anyhow!("hash mismatch"));
//             }

//             self.file
//                 .lock()
//                 .unwrap()
//                 .write_all_at(
//                     &cur_state.piece.lock().unwrap(),
//                     cur_state.piece_i as u64 * self.torrent.info.piece_length as u64,
//                 )
//                 .context("write piece")?;
//             let file_info = &mut self.file_info.lock().unwrap();
//             let new_bitfield = file_info.bitfield.to_mut();
//             new_bitfield[cur_state.piece_i as usize] = true;

//             println!("piece {} complete", cur_state.piece_i);

//             new_bitfield.to_vec().into()
//         };

//         let updated: Option<FileInfo> = self
//             .db_conn
//             .update(("files", self.get_info_hash()))
//             .patch(PatchOp::replace("/bitfield", new_bitfield))
//             .await?;
//         dbg!(&updated);

//         if updated.is_none() {
//             panic!("No record was found");
//         }

//         if self.file_info.lock().unwrap().is_finished() {
//             println!("file is finished");
//             self.download_state.lock().unwrap().clear();
//             return Ok(());
//         }

//         Ok(())
//     }

//     fn get_info_hash(&self) -> String {
//         self.info_hash.iter().collect()
//     }

//     /// prepares the DownloadState for the next piece
//     /// it takes i_have as an argument (I could have just locked the Mutex) because
//     /// the mutex is already locked in the caller (`prepare_next_req_send`)
//     fn get_next_download_state(&self, peer_has: &[bool], i_have: &[bool]) -> Option<DownloadState> {
//         let new_piece_i = self.choose_next_piece(peer_has, i_have);

//         if let Some(new_piece_i) = new_piece_i {
//             let piece_size = self.get_piece_size(new_piece_i);

//             Some(DownloadState {
//                 piece_i: new_piece_i,
//                 block_i: 0,
//                 piece: Arc::new(Mutex::new(Vec::with_capacity(piece_size as usize))),
//             })
//         } else {
//             None
//         }
//     }

//     fn choose_next_piece(&self, peer_has: &[bool], i_have: &[bool]) -> Option<u32> {
//         let mut rng = rand::rng();
//         peer_has
//             .iter()
//             .enumerate()
//             .zip(i_have.iter().enumerate())
//             .filter_map(|((i_peer, b_p), (i_self, b_s))| {
//                 if !*b_s && *b_p {
//                     // essentially I put both the bitfields toghether (which are in equal length)
//                     assert_eq!(i_peer, i_self);
//                     Some(i_peer as u32)
//                 } else {
//                     None
//                 }
//             })
//             .choose(&mut rng)
//     }
//     /// returns piece_size and n_blocks
//     fn get_piece_size(&self, piece_i: u32) -> u32 {
//         let torrent = &self.torrent;
//         let length = torrent.get_length();
//         if piece_i == torrent.info.pieces.0.len() as u32 - 1
//             && length % torrent.info.piece_length != 0
//         {
//             length % torrent.info.piece_length
//         } else {
//             torrent.info.piece_length
//         }
//     }
// }

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
