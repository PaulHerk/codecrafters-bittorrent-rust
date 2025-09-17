use sha1::{Digest, Sha1};
use surrealdb::{Surreal, engine::local::Db};
use tokio::fs::File;

use super::{PieceState, ReqManager};
use crate::{BLOCK_MAX, RequestPiecePayload, ResponsePiecePayload, Torrent};

impl ReqManager {
    /// writes a block to the buffer
    /// if it's the last block of a piece, it checks the hash, updates the bitfield and writes the piece to the file
    /// returns Some index of the piece if it's finished
    pub(super) async fn write_block(&mut self, block: ResponsePiecePayload) -> Option<u32> {
        let Some(download_queue) = self.download_state.as_mut() else {
            // we're finished
            return None;
        };
        let piece_state = download_queue
            .iter_mut()
            .find(|s| s.piece_i == block.index)
            .unwrap_or_else(|| {
                todo!("create new state for piece if the size is less that MAX_PIECES_IN_PARALLEL");
            });

        println!("got block");
        piece_state.update_state(block);
        if piece_state.bitfield.iter().all(|b| *b) {
            // we're done with this piece
            piece_state.check_hash(&self.torrent);
            piece_state.write_to_file(&mut self.file).await;
            piece_state.update_db_entry(&self.db_conn).await;

            todo!(
                "remove piece_state from download queue, I cannot do pop_front because it might be that the front one is slow and the last one finishes earlier"
            );
            // cannot do: states.pop_front();
            // TODO: write block to file
            todo!("write piece");

            Some(piece_state.piece_i)
        } else {
            None
        }
    }

    /// returns a block a peer requested
    pub(super) fn get_block(&self, req_payload: RequestPiecePayload) -> ResponsePiecePayload {
        todo!()
    }

    pub(super) fn get_have(&self) -> Vec<bool> {
        todo!("assert that self.have is the same as the DB");
        todo!("return self.have");
        self.have.clone()
    }
}

impl PieceState {
    fn update_state(&mut self, block: ResponsePiecePayload) {
        self.buf.extend_from_slice(&block.block);

        let block_i = block.begin as usize / BLOCK_MAX as usize;
        assert!(block_i < self.bitfield.len());
        self.bitfield.insert(block_i, true);
    }

    fn check_hash(&self, torrent: &Torrent) {
        let mut sha1 = Sha1::new();
        sha1.update(&self.buf);
        let hash: [u8; 20] = sha1.finalize().into();
        let torrent_hash = torrent.info.pieces.0[self.piece_i as usize];
        if hash != torrent_hash {
            todo!("handle hash-mismatch error");
        }
    }

    async fn write_to_file(&self, file: &mut File) {
        todo!("write to file");
    }

    async fn update_db_entry(&self, db_conn: &Surreal<Db>) {
        todo!("update db entry");
    }
}
