use std::{io::SeekFrom, os::unix::fs::FileExt};

use anyhow::Context;
use sha1::{Digest, Sha1};

use super::{PieceState, ReqManager};
use crate::{
    BLOCK_MAX, RequestPiecePayload, ResponsePiecePayload, Torrent, req_manager::BlockState,
};

impl ReqManager {
    /// writes a block to the buffer
    /// handles the piece if it's the last one
    /// returns Some index of the piece if it's finished
    pub(super) async fn write_block(
        &mut self,
        block: ResponsePiecePayload,
    ) -> anyhow::Result<Option<u32>> {
        let Some(download_queue) = self.download_queue.as_mut() else {
            // we're finished
            return Ok(None);
        };
        let Some((queue_i, piece_state)) = download_queue
            .iter_mut()
            .enumerate()
            .find(|(_i, s)| s.piece_i == block.index)
        else {
            // if the piece isn't even something we want we ignore it
            // TODO: we might aswell add a new PieceState to the download_queue if it's not full yet
            // self.add_piece_to_queue();
            // recursion not really ideal
            return Ok(None);
        };

        println!("got block");
        piece_state.update_state(block);
        if piece_state.blocks.iter().all(|b| b.is_finished()) {
            println!("piece {} is done", piece_state.piece_i);
            // we're done with this piece
            let piece_state = download_queue.remove(queue_i).expect("Index exists.");
            self.handle_piece(&piece_state).await?;

            Ok(Some(piece_state.piece_i))
        } else {
            Ok(None)
        }
    }

    // it checks the hash, updates the bitfield and writes the piece to the file
    // if this fails somewhere, it should be fine since the piece will get picked up later again
    async fn handle_piece(&mut self, piece_state: &PieceState) -> anyhow::Result<()> {
        piece_state.check_hash(&self.torrent)?;
        self.write_piece_to_file(piece_state).await?;

        // we first calculate the new bitfield, then update it in the DB and lastly update the struct
        // this is so if the DB fails, the struct is still in the old state
        let mut new_bitfield = self.have.clone();
        let piece_i = piece_state.piece_i as usize;
        new_bitfield[piece_i] = true;
        self.db_conn
            .update_bitfields(&self.info_hash, new_bitfield)
            .await?;
        self.have[piece_i] = true;

        Ok(())
    }

    async fn write_piece_to_file(&mut self, piece_state: &PieceState) -> anyhow::Result<()> {
        let offset = piece_state.piece_i as u64 * self.torrent.info.piece_length as u64;

        let mut buf = &piece_state.buf[..];
        self.file
            .write_all_at(&mut buf, offset)
            .context("writing piece to file")?;

        Ok(())
    }

    /// returns a block a peer requested
    pub(super) fn get_block(&self, req_payload: RequestPiecePayload) -> ResponsePiecePayload {
        todo!()
    }
}

impl PieceState {
    fn update_state(&mut self, block: ResponsePiecePayload) {
        let block_len = block.block.len();
        let block_begin = block.begin as usize;
        let block_end = block_begin + block_len;
        self.buf[block_begin..block_end].copy_from_slice(&block.block);

        let block_i = block.begin as usize / BLOCK_MAX as usize;
        assert!(block_i < self.blocks.len());
        self.blocks[block_i] = BlockState::Finished;
    }

    fn check_hash(&self, torrent: &Torrent) -> Result<(), anyhow::Error> {
        let mut sha1 = Sha1::new();
        sha1.update(&self.buf);
        let hash: [u8; 20] = sha1.finalize().into();
        let torrent_hash = torrent.info.pieces.0[self.piece_i as usize];
        if hash != torrent_hash {
            Err(anyhow::anyhow!("Hash Mismatch"))
        } else {
            Ok(())
        }
    }
}
