use crate::RequestPiecePayload;

use super::ReqManager;

impl ReqManager {
    /// returns a list of blocks that we want to request
    pub(super) fn prepare_next_blocks(
        &self,
        n: usize,
        peer_has: Vec<bool>,
    ) -> Vec<RequestPiecePayload> {
        todo!()
    }

    /// checks whether the queue is full, if not adds a new item
    /// returns whether a new piece is added (true) or not (false)
    pub(super) fn add_piece_to_queue(&mut self) -> bool {
        todo!()
    }
}
