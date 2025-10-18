use bytes::BytesMut;
use rand::seq::IteratorRandom;

use crate::{
    BLOCK_MAX,
    messages::payloads::RequestPiecePayload,
    peer_manager::{BlockState, MAX_PIECES_IN_PARALLEL, PeerId, PieceManager, PieceState},
    torrent::Metainfo,
};

#[derive(Debug)]
pub(super) struct DownloadQueue(pub(super) Vec<PieceState>);

impl PieceManager {
    /// returns a list of blocks that we want to request
    pub(in crate::peer_manager) fn prepare_next_blocks(
        &mut self,
        n: usize,
        peer_id: &PeerId,
        metainfo: &Metainfo,
    ) -> Option<Vec<RequestPiecePayload>> {
        let piece = if let Some(peer_has) = self.piece_selector.get_peer_has(peer_id) {
            if let Some(piece) = self.download_queue.get_queue_for_peer(peer_has) {
                Some(piece)
            } else {
                // a bit awkard tbh
                let peer_has = peer_has.clone();

                if let Some(queue) = self
                    .piece_selector
                    .select_pieces_for_peer(peer_id, 3 /* TODO */)
                {
                    self.download_queue.add_pieces_to_queue(queue, metainfo);
                }
                self.download_queue.get_queue_for_peer(&peer_has)
            }
        } else {
            None
        }?;

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

        Some(requests)
    }
}

impl DownloadQueue {
    pub(super) fn new() -> Self {
        Self(Vec::with_capacity(MAX_PIECES_IN_PARALLEL))
    }

    fn get_queue_for_peer(&mut self, peer_has: &[bool]) -> Option<&mut PieceState> {
        // 1. Try if we have something in the download queue
        let piece_i = self.0.iter().position(|state| {
            let Some(peer_has_it) = peer_has.get(state.piece_i as usize) else {
                // I've had an occurrence where this failed
                dbg!("WOAHUFHAKFLA:SFUIOHAF:IUHA:FIUAFIUB");
                return false;
            };
            let blocks_we_need = state.blocks.iter().filter(|b| b.is_none());
            // TODO: now currently if there's only one block remaining in the queue, it will return only that one
            // we might want to return that plus like 9 more of the next piece
            *peer_has_it && blocks_we_need.count() >= 1
        })?;
        // TODO: if we didn't find the piece here, we'll run the piece_selector and call this function again
        // however it has to recompute the pieces so we might want to find a better solution here

        // the download_queue will have a last piece, because it may have been added by self.add_piece_to_queue. If it hasn't, we have returned.
        Some(self.0.get_mut(piece_i).expect("we checked that before"))
    }

    /// checks whether the queue is full, if not adds a new item
    /// returns whether a new piece is added (true) or not (false)
    pub(in crate::peer_manager) fn add_pieces_to_queue(
        &mut self,
        pieces: Vec<u32>,
        metainfo: &Metainfo,
    ) {
        for piece_i in pieces.into_iter() {
            let piece_state = PieceState::new(metainfo, piece_i);
            self.0.push(piece_state);
        }
    }
}

impl PieceState {
    /// calculates n_blocks and piece_size and creates a new PieceState
    pub(in crate::peer_manager) fn new(torrent_info: &Metainfo, piece_i: u32) -> Self {
        let piece_size = get_piece_size(torrent_info, piece_i);
        let n_blocks = piece_size.div_ceil(BLOCK_MAX);

        PieceState {
            blocks: vec![BlockState::None; n_blocks as usize],
            piece_i,
            buf: BytesMut::zeroed(piece_size as usize),
        }
    }
}

fn get_piece_size(torrent_info: &Metainfo, piece_i: u32) -> u32 {
    let length = torrent_info.get_length();
    let piece_length = torrent_info.piece_length;
    if piece_i == torrent_info.pieces.0.len() as u32 - 1 && length % piece_length != 0 {
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
