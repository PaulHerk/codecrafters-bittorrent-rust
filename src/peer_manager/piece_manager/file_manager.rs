use std::os::unix::fs::FileExt;

use bytes::BytesMut;
use sha1::{Digest, Sha1};

use super::PieceState;
use crate::{
    BLOCK_MAX,
    messages::payloads::{RequestPiecePayload, ResponsePiecePayload},
    peer_manager::{
        BlockState, PieceManager, error::PeerManagerError, piece_manager::DownloadQueue,
    },
    torrent::Metainfo,
};

impl PieceManager {
    /// writes a block to the buffer
    /// handles the piece if it's the last one
    /// returns Some index of the piece if it's finished
    pub(in crate::peer_manager) async fn write_block(
        &mut self,
        block: ResponsePiecePayload,
        metainfo: &Metainfo,
    ) -> Result<Option<u32>, PeerManagerError> {
        if let Some(piece_state) = self.download_queue.update_piece_state(block) {
            self.handle_piece(&piece_state, metainfo).await?;
            Ok(Some(piece_state.piece_i))
        } else {
            Ok(None)
        }
    }

    // it checks the hash, updates the bitfield and writes the piece to the file
    // if this fails somewhere, it should be fine since the piece will get picked up later again
    async fn handle_piece(
        &mut self,
        piece_state: &PieceState,
        metainfo: &Metainfo,
    ) -> Result<(), PeerManagerError> {
        let hashs_match = piece_state.check_hash(metainfo);
        if !hashs_match {
            return Ok(());
        }
        self.write_piece_to_file(piece_state, metainfo).await?;

        // we first calculate the new bitfield, then update it in the DB and lastly update the struct
        // this is so if the DB fails, the struct is still in the old state
        let mut new_bitfield = self.have.clone();
        let piece_i = piece_state.piece_i as usize;
        new_bitfield[piece_i] = true;
        self.db_conn.update_bitfields(new_bitfield).await?;
        self.have[piece_i] = true;

        Ok(())
    }

    async fn write_piece_to_file(
        &mut self,
        piece_state: &PieceState,
        metainfo: &Metainfo,
    ) -> Result<(), PeerManagerError> {
        let offset = piece_state.piece_i as u64 * metainfo.piece_length as u64;

        let buf = &piece_state.buf[..];
        self.file.write_all_at(buf, offset)?;

        Ok(())
    }

    /// returns a block a peer requested
    pub(in crate::peer_manager) fn get_block(
        &self,
        req_payload: RequestPiecePayload,
        metainfo: &Metainfo,
    ) -> Option<ResponsePiecePayload> {
        let mut buf = BytesMut::zeroed(req_payload.length as usize);
        let offset =
            req_payload.index as u64 * metainfo.piece_length as u64 + req_payload.begin as u64;
        if self.file.read_exact_at(&mut buf, offset).is_err() {
            return None;
        }

        let res = ResponsePiecePayload {
            index: req_payload.index,
            begin: req_payload.begin,
            block: buf.freeze(),
        };
        Some(res)
    }

    pub(in crate::peer_manager) fn is_finished(&self) -> bool {
        self.have.iter().all(|b| *b)
    }
}

impl DownloadQueue {
    /// function that updates the PieceState in the queue in response to a payload
    /// also if we're done with the piece, it gets removed and returned from the queue
    fn update_piece_state(&mut self, block: ResponsePiecePayload) -> Option<PieceState> {
        let (queue_i, piece_state) = self
            .0
            .iter_mut()
            .enumerate()
            .find(|(_i, s)| s.piece_i == block.index)?;
        // if the piece isn't even something we want we ignore it
        // TODO: we might aswell add a new PieceState to the download_queue if it's not full yet
        // self.add_piece_to_queue();
        // recursion not really ideal

        piece_state.update_state(block);
        if piece_state.blocks.iter().all(|b| b.is_finished()) {
            // we're done with this piece
            Some(self.0.swap_remove(queue_i))
        } else {
            None
        }
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

    fn check_hash(&self, torrent_info: &Metainfo) -> bool {
        let mut sha1 = Sha1::new();
        sha1.update(&self.buf);
        let hash: [u8; 20] = sha1.finalize().into();
        let torrent_hash = torrent_info.pieces.0[self.piece_i as usize];
        hash == torrent_hash
    }
}
