use rand::seq::IndexedRandom;

use crate::{
    BLOCK_MAX, Torrent,
    messages::payloads::RequestPiecePayload,
    peer_manager::{BlockState, MAX_PIECES_IN_PARALLEL, PieceState},
};

use super::PeerManager;

impl PeerManager {
    /// returns a list of blocks that we want to request
    pub(super) fn prepare_next_blocks(
        &mut self,
        n: usize,
        peer_has: Vec<bool>,
    ) -> Vec<RequestPiecePayload> {
        let Some(download_queue) = &self.download_queue else {
            return Vec::new();
        };

        // 1. Try if we have something in the download queue
        let piece_i = download_queue.iter().position(|state| {
            let peer_has_it = peer_has[state.piece_i as usize];
            let blocks_we_need = state.blocks.iter().filter(|b| b.is_none());
            // TODO: now currently if there's only one block remaining in the queue, it will return only that one
            // we might want to return that plus like 9 more of the next piece
            peer_has_it && blocks_we_need.count() >= 1
        });

        // 2. If not, add something to the queue: realistically rarest-first
        if piece_i.is_none() && !self.add_piece_to_queue(&peer_has) {
            return Vec::new();
        }
        drop(peer_has);

        // we also need new mutable access to the download_queue since self might have changed above
        let download_queue_mut = self
            .download_queue
            .as_mut()
            .expect("we checked that before");

        let piece_i = piece_i.or(download_queue_mut.len().checked_sub(1)).expect(
                "the download_queue will have a last piece, because it may have been added by self.add_piece_to_queue. If it hasn't, we have returned."
            );
        let piece = download_queue_mut
            .get_mut(piece_i)
            .expect("we checked that before");

        // 3. Actually create the responses
        let mut requests = Vec::with_capacity(n);
        let n_blocks = piece.blocks.capacity() as u32;
        let piece_size = piece.buf.capacity() as u32;

        for (block_i, block) in piece
            .blocks
            .iter_mut()
            .enumerate()
            .filter(|(_, b)| b.is_none())
            .take(n)
        {
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
        let Some(queue) = &mut self.download_queue else {
            return false;
        };

        // if the queue is already to big but we're at the last piece, we still want to add it
        if queue.len() == MAX_PIECES_IN_PARALLEL && self.have.iter().filter(|b| **b).count() > 1 {
            return false;
        }

        let possible_pieces: Vec<u32> = self
            .have
            .iter()
            .zip(peer_has)
            .enumerate()
            .filter_map(|(index, (i_have, p_has))| {
                if !*i_have && *p_has && !queue.iter().any(|s| s.piece_i == index as u32) {
                    Some(index as u32)
                } else {
                    None
                }
            })
            .collect();

        let Some(piece_i) = possible_pieces.choose(&mut rand::rng()) else {
            return false;
        };

        let piece_state = PieceState::new(&self.torrent, *piece_i);
        queue.push(piece_state);

        true
    }
}

impl PieceState {
    /// calculates n_blocks and piece_size and creates a new PieceState
    pub(super) fn new(torrent: &Torrent, piece_i: u32) -> Self {
        let piece_size = get_piece_size(torrent, piece_i);
        let n_blocks = piece_size.div_ceil(BLOCK_MAX);

        PieceState {
            blocks: vec![BlockState::None; n_blocks as usize],
            piece_i,
            buf: vec![0; piece_size as usize],
        }
    }
}

fn get_piece_size(torrent: &Torrent, piece_i: u32) -> u32 {
    let length = torrent.get_length();
    let piece_length = torrent.info.piece_length;
    if piece_i == torrent.info.pieces.0.len() as u32 - 1 && length % piece_length != 0 {
        length % piece_length
    } else {
        piece_length
    }
}

fn get_block_len(n_blocks: u32, piece_size: u32, block_i: u32) -> u32 {
    if block_i == n_blocks - 1 && piece_size % BLOCK_MAX != 0 {
        piece_size % BLOCK_MAX
    } else {
        BLOCK_MAX
    }
}
