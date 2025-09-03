use std::{
    os::unix::fs::FileExt,
    sync::{Arc, Mutex},
};

use anyhow::Context;
use rand::seq::IteratorRandom;
use sha1::{Digest, Sha1};
use tokio::sync::broadcast::Receiver;

use crate::{BLOCK_MAX, HavePayload, RequestPiecePayload, ResponsePiecePayload, Torrent};

/// this struct is passed to all the peers
#[derive(Debug, Clone)]
pub struct PeerData {
    torrent: Arc<Mutex<Torrent>>,
    /// multiple pieces
    /// each piece is a Vec of blocks
    /// each block is a Vec of bytes
    pieces: Arc<Mutex<Vec<Vec<Vec<u8>>>>>,
    have: Arc<Mutex<Vec<PieceState>>>,
    /// it's None if we're seeding
    current_piece: Arc<Mutex<Option<u32>>>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum PieceState {
    Incomplete(Vec<BlockState>),
    Complete,
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum BlockState {
    None,
    InProgress,
    Complete,
}

impl PieceState {
    fn is_complete(&self) -> bool {
        match self {
            PieceState::Complete => true,
            PieceState::Incomplete(blocks) => blocks.iter().all(|b| *b == BlockState::Complete),
        }
    }
}

impl PeerData {
    pub fn new(torrent: Torrent, bytes: &[u8]) -> Self {
        let length = torrent.get_length();

        let mut pieces: Vec<Vec<Vec<u8>>> = Vec::with_capacity(torrent.info.pieces.0.len());
        let mut have = Vec::with_capacity(torrent.info.pieces.0.len());

        // TODO: store a bitfield in memory
        let mut current_piece = None;

        // what this loop does:
        // 1. put the bytes into the pieces Vec
        // 2. allocate the exact space needed if the bytes are yet to be recieved
        // 3. initialize the have Vec
        // TODO: the last 2 steps are absolutely not necessarily. We should just store a bitfield in memory
        for piece_i in 0..torrent.info.pieces.0.len() as u32 {
            let piece_size = if piece_i == torrent.info.pieces.0.len() as u32 - 1
                && length % torrent.info.piece_length != 0
            {
                length % torrent.info.piece_length
            } else {
                torrent.info.piece_length
            };
            // the "+ BLOCK_MAX - 1" rounds up
            let nblocks = (piece_size + BLOCK_MAX - 1) / BLOCK_MAX;
            // eprintln!("{nblocks} blocks of at most {BLOCK_MAX} to reach {piece_size}");

            pieces.push(Vec::with_capacity(piece_size as usize));
            have.push(PieceState::Incomplete(vec![
                BlockState::None;
                nblocks as usize
            ]));

            for block_i in 0..nblocks {
                let block_length = if block_i == nblocks - 1 && piece_size % BLOCK_MAX != 0 {
                    piece_size % BLOCK_MAX
                } else {
                    BLOCK_MAX
                };

                pieces[piece_i as usize].push(Vec::with_capacity(block_length as usize));
            }
            // TODO: it should now check a bitfield vec in memory to see if we have the block
            // and if we do, update have
            let _current_have = have.get_mut(piece_i as usize).expect("we just pushed it");
        }

        Self {
            torrent: Arc::new(Mutex::new(torrent)),
            pieces: Arc::new(Mutex::new(pieces)),
            have: Arc::new(Mutex::new(have)),
            current_piece: Arc::new(Mutex::new(current_piece)),
        }
    }
    /// returns the next piece and block begin that we need to request
    /// if we have all the blocks of a piece, we return None
    pub(super) fn prepare_next_req_send(&self, peer_has: &[bool]) -> Option<RequestPiecePayload> {
        let have = &mut self.have.lock().unwrap();

        let current_piece = self
            .current_piece
            .lock()
            .unwrap()
            .context("no current piece")
            .ok()? as usize;
        let current_piece_state = have
            .get_mut(current_piece)
            .expect("it must be there since we set it in a previous iteration of this function");

        // TODO: if peer doesn't have piece or if it's complete *but* not all pieces are complete we recalculate and call this function again
        let (piece_i, blocks_state) =
            if let PieceState::Incomplete(piece_state) = current_piece_state {
                (current_piece, piece_state)
            } else {
                println!("recalculating");

                let all_pieces = have.iter_mut().zip(peer_has.iter());
                let mut rng = rand::rng();

                let piece_i_and_blocks = all_pieces
                    .enumerate()
                    .filter_map(|(piece_i, (piece_state, peer_has))| match piece_state {
                        PieceState::Incomplete(blocks)
                            if *peer_has
                                && blocks.iter().find(|b| *b == &BlockState::None).is_some() =>
                        {
                            Some((piece_i, blocks))
                        }
                        _ => None,
                    })
                    .choose(&mut rng);

                *self.current_piece.lock().unwrap() = piece_i_and_blocks
                    .as_ref()
                    .and_then(|(piece_i, _)| Some(*piece_i as u32));

                let Some((piece_i, blocks_state)) = piece_i_and_blocks else {
                    return None;
                };

                (piece_i, blocks_state)
            };
        // TODO: this is not necessarily performant since we filter all elements but the just choose one

        let (block_i, block_state) = blocks_state
            .iter_mut()
            .enumerate()
            .find(|(_i, b)| *b == &BlockState::None)
            .expect("there should be an empty if there's a piece that is incomplete");
        // TODO: same as above
        let block_i = block_i as u32;
        let block_begin = block_i * BLOCK_MAX;
        let block_len = self.pieces.lock().unwrap()[piece_i][block_i as usize].capacity() as u32;

        *block_state = BlockState::InProgress;

        let req = RequestPiecePayload::new(piece_i as u32, block_begin, block_len);

        Some(req)
    }

    /// returns true if a piece was completed (hashs match)
    pub(super) fn add_block(&self, payload: ResponsePiecePayload) -> Option<u32> {
        // the "+ BLOCK_MAX - 1" rounds up
        let block_i = (payload.begin + BLOCK_MAX - 1) / BLOCK_MAX;

        let piece = &mut self.pieces.lock().unwrap()[payload.index as usize];
        piece[block_i as usize].append(&mut payload.block.to_vec());

        let have = &mut self.have.lock().unwrap()[payload.index as usize];
        let PieceState::Incomplete(blocks_state) = have else {
            unreachable!()
        };
        let nblocks = blocks_state.len();
        blocks_state[block_i as usize] = BlockState::Complete;
        eprintln!("block {} of piece {} is complete", block_i, payload.index);

        // calculate the hash of the torrent if we have a piece
        let torrent = self.torrent.lock().unwrap();
        let is_complete = have.is_complete() && have != &PieceState::Complete;
        if is_complete.clone() {
            // the second condition should always be false
            let mut sha1 = Sha1::new();
            sha1.update(piece.concat());
            let hash: [u8; 20] = sha1
                .finalize()
                .try_into()
                .expect("GenericArray<_, 20> == [u8; 20]");
            let torrent_hash = torrent.info.pieces.0[payload.index as usize];
            if hash != torrent_hash {
                piece[block_i as usize].clear();
                *have = PieceState::Incomplete(vec![BlockState::None; nblocks]);
                return None;
            }

            println!("piece {} complete", payload.index);
            *have = PieceState::Complete;
            // TODO: send have msg
            return Some(payload.index);
        }
        None
    }

    pub async fn write_file(
        &self,
        file: std::fs::File,
        mut rx: Receiver<HavePayload>,
    ) -> anyhow::Result<()> {
        let total_pieces = self.pieces.lock().unwrap().len() as u32;
        let piece_length = self.torrent.lock().unwrap().info.piece_length;

        let mut pieces_recv: u32 = 0;
        while let Ok(have) = rx.recv().await {
            let piece = &self.pieces.lock().unwrap()[have.piece_index as usize];
            file.write_all_at(
                piece.concat().as_slice(),
                have.piece_index as u64 * piece_length as u64,
            )
            .context("write piece")?;
            pieces_recv += 1;
            if pieces_recv == total_pieces {
                break;
            }
        }
        assert_eq!(
            file.metadata().context("get file metadata")?.len(),
            self.torrent.lock().unwrap().get_length() as u64
        );
        println!("File download complete.");
        Ok(())
    }

    pub(super) fn get_block(&self, request_piece_payload: RequestPiecePayload) -> Option<Vec<u8>> {
        let piece_i = request_piece_payload.index as usize;
        let begin = request_piece_payload.begin as usize;
        let length = request_piece_payload.length as usize;

        if self.have.lock().unwrap()[piece_i] != PieceState::Complete {
            return None;
        }

        let block = self.pieces.lock().unwrap()[piece_i].concat();
        let block = block
            .get(begin..begin + length)
            .and_then(|b| Some(b.to_vec()));
        // TODO: rn it's concatonating a whole piece which might not be so efficient

        block
    }
}
