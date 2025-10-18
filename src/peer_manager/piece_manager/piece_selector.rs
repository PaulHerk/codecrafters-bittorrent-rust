//! This whole thing is merely existent for selecting the next pieces we might send to peers.
//! The actual job of creating requests, handling timeouts is done in the `req_preparer.rs`

use std::{
    cmp::Reverse,
    collections::{BinaryHeap, HashMap},
    vec,
};

use crate::peer_manager::PeerId;

// 1.  We **keep** the `piece_rarity: Vec<u32>` as the single source of truth for piece rarity.
// 2.  When a piece's rarity is updated (e.g., a peer connects and has the piece), we simply **push a new entry** `(new_rarity, piece_index)` into the `BinaryHeap`. We do **not** remove the old entry.
// 3.  When we call `select_pieces_for_peer`, we pop an item (rarity_in_heap, piece_index)` from the heap.
// 4.  **Crucially, we then check if `rarity_in_heap` matches the current, true rarity in `self.piece_rarity[piece_index]`.**
//     *   If they don't match, it means this heap entry is **stale**. We discard it and pop the next one.
//     *   If they match, this is a valid, up-to-date entry. We process it.

// This "lazy" approach has some great benefits:
// *   **Correctness:** We only ever act on the most up-to-date rarity information.
// *   **Simplicity:** We only use the standard library's `BinaryHeap` and a `Vec`. No complex data structures needed.
// *   **Good Performance:** The heap might grow larger with stale entries, but heap operations are logarithmic (`log N`). In practice, the performance is excellent, and the stale entries are cleaned out as they are encountered. The cost of pushing a new entry is small.
#[derive(Debug)]
pub(in crate::peer_manager) struct PieceSelector {
    /// A vector where the index is the piece index and the value
    /// is the number of peers that have this piece.
    piece_rarity: Vec<u32>,

    /// A priority queue of pieces to download.
    /// The tuple is (rarity, piece_index).
    // (a,b)>(c,d) if a>c OR (a=c AND b>d) which makes this work as it should
    priority_queue: BinaryHeap<Reverse<(u32, u32)>>,

    /// Our own bitfield, to know which pieces we already have.
    pub(super) have: Vec<bool>,

    /// A map to store the bitfield for each peer.
    /// The key is the PeerId.
    peer_bitfields: HashMap<PeerId, Vec<bool>>,

    /// Tracks pieces that are currently in the DownloadQueue.
    /// This prevents us from selecting the same piece multiple times.
    /// A Vec<bool> is likely more efficient than a HashSet<u32> if piece indices are contiguous.
    pieces_in_flight: Vec<bool>,
}

impl PieceSelector {
    pub(in crate::peer_manager) fn new(have: Vec<bool>) -> Self {
        let n_piceces = have.len();
        Self {
            piece_rarity: vec![0; n_piceces],
            priority_queue: BinaryHeap::with_capacity(n_piceces),
            have,
            peer_bitfields: HashMap::new(),
            pieces_in_flight: vec![false; n_piceces],
        }
    }

    pub(in crate::peer_manager) fn add_peer(&mut self, id: PeerId) {
        if let Some(old) = self.peer_bitfields.insert(id, Vec::new()) {
            // this shouldn't happen, but if it does we're safe
            self.peer_bitfields.insert(id, old);
        };
    }

    /// removes the peer from the list and the priority queue
    pub(in crate::peer_manager) fn remove_peer(&mut self, id: &PeerId) {
        if let Some(bitfield) = self.peer_bitfields.remove(id) {
            self.update_prio_queue(bitfield, false);
        }
    }

    /// updates the priority queue if the peer sent us his bitfield
    /// if the piece is not added via `add_peer` yet, it will do nothing
    pub(in crate::peer_manager) fn add_bitfield(&mut self, id: &PeerId, bitfield: Vec<bool>) {
        if let Some(peer_bitfield) = self.peer_bitfields.get_mut(id) {
            *peer_bitfield = bitfield.clone();
            self.update_prio_queue(bitfield, true);
        }
    }

    /// updates the priority queue if the peer sent us a have message
    pub(in crate::peer_manager) fn update_have(&mut self, id: &PeerId, have_index: u32) {
        // for easier indexing
        let i = have_index as usize;
        if let Some(peer_bitfield) = self.peer_bitfields.get_mut(id)
            && let Some(b_peer) = peer_bitfield.get_mut(i)
            && let Some(rarity) = self.piece_rarity.get_mut(i)
        {
            *b_peer = true;
            *rarity += 1;
            if !self.have[i] {
                self.priority_queue.push(Reverse((*rarity, have_index)));
            }
        }
    }

    // checks in order:
    // - (does bitfield for peer exist)
    // - is there a next piece from the priority queue
    // - (if yes) is the piece even up-to-date
    // - (if yes) does the peer even have that piece
    // - (if yes) is the piece already requested by any peer
    /// selects `count` amount of pieces from a peer so he can request them
    pub(super) fn select_pieces_for_peer(&mut self, id: &PeerId, count: usize) -> Option<Vec<u32>> {
        let Some(bitfield) = self.peer_bitfields.get(id) else {
            return None;
        };
        let mut queue = Vec::with_capacity(count);
        while queue.len() <= queue.capacity()
            && let Some(Reverse((count, piece_i))) = self.priority_queue.pop()
        {
            let i = piece_i as usize;
            // the priority queue doesn't have to be up to date since we don't remove values if we receive e.g. a have message
            if self.piece_rarity[i] == count && bitfield[i] && !self.pieces_in_flight[i] {
                queue.push(piece_i);
                self.pieces_in_flight[i] = true;
            }
        }
        if queue.is_empty() { None } else { Some(queue) }
    }

    /// updates both priority queues with a bitfield and whether it should increase or decrease the count
    fn update_prio_queue(&mut self, bitfield: impl IntoIterator<Item = bool>, increase: bool) {
        self.piece_rarity
            .iter_mut()
            .enumerate()
            .zip(bitfield)
            .zip(&self.have)
            .for_each(|(((i, count), b_p), b_i)| {
                if b_p && !b_i {
                    match increase {
                        true => *count += 1,
                        false => *count = count.saturating_sub(1),
                    }
                    self.priority_queue.push(Reverse((*count, i as u32)));
                }
            });
        dbg!(&self.priority_queue);
    }

    pub(in crate::peer_manager) fn get_peer_has(&self, id: &PeerId) -> Option<&Vec<bool>> {
        self.peer_bitfields.get(id)
    }
}
