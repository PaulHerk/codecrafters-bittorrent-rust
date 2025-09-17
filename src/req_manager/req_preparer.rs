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
}
