use std::ops::Deref;

use crate::{
    BLOCK_MAX, BLOCK_QUEUE_SIZE_MAX, RequestPiecePayload,
    req_manager::{BLOCK_QUEUE_SIZE_MIN, BlockState, PieceState},
};

use super::ReqManager;

impl ReqManager {
    /// returns a list of blocks that we want to request
    pub(super) fn prepare_next_blocks(
        &mut self,
        n: usize,
        peer_has: Vec<bool>,
    ) -> Vec<RequestPiecePayload> {
        let Some(download_queue) = &mut self.download_queue else {
            return Vec::new();
        };

        // 1. try if we have something in the download queue
        // 2. if not, add something to the queue: realistically rarest-first
        // 3. proceed

        let piece = download_queue
            .iter_mut()
            .find(|state| {
                let peer_has_it = peer_has[state.piece_i as usize] == true;
                let blocks_we_need = state.blocks.iter().filter(|b| b.is_none()).count();
                return peer_has_it && blocks_we_need >= BLOCK_QUEUE_SIZE_MIN;
            })
            .cloned();
        if let None = piece {
            self.add_piece_to_queue(&peer_has);
        }
        let Some(download_queue) = &mut self.download_queue else {
            unreachable!("we checked that before")
        };
        let piece = piece.or(download_queue.back_mut().cloned());

        let Some(mut piece) = piece else {
            return Vec::new();
        };

        let mut requests = Vec::with_capacity(n);
        let n_blocks = piece.blocks.capacity() as u32;
        let piece_size = piece.buf.capacity() as u32;

        for (block_i, block) in piece.blocks.iter_mut().enumerate() {
            let block_i = block_i as u32;
            let index = piece.piece_i;
            let begin = block_i * BLOCK_MAX;
            let length = get_block_len(n_blocks, piece_size, block_i);
            let req = RequestPiecePayload::new(index, begin, length);
            requests.push(req);

            *block = BlockState::InProcess;
        }

        requests
    }

    /// checks whether the queue is full, if not adds a new item
    /// returns whether a new piece is added (true) or not (false)
    pub(super) fn add_piece_to_queue(&mut self, peer_has: &Vec<bool>) -> bool {
        todo!()
    }
}

fn get_block_len(n_blocks: u32, piece_size: u32, block_i: u32) -> u32 {
    if block_i == n_blocks - 1 && piece_size % BLOCK_MAX != 0 {
        piece_size % BLOCK_MAX
    } else {
        BLOCK_MAX
    }
}
