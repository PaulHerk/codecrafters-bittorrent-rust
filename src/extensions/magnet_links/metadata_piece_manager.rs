use std::io::{self, Cursor, Write};

use crate::{messages::PeerMessage, torrent::InfoHash};

/// The metadata is handled in blocks of 16KiB (16384 Bytes).
const METADATA_BLOCK_SIZE: usize = 16384;

pub(crate) struct MetadataPieceManager {
    queue: Vec<bool>,
    bytes: Cursor<Vec<u8>>,
    info_hash: InfoHash,
}

impl MetadataPieceManager {
    pub(crate) fn new(info_hash: InfoHash) -> Self {
        Self {
            queue: Vec::new(),
            bytes: Cursor::new(Vec::with_capacity(METADATA_BLOCK_SIZE * 10)), // let's just allocate enough space
            info_hash,
        }
    }

    pub(crate) fn add_block(&mut self, index: u32, data: Vec<u8>) -> io::Result<()> {
        self.bytes.set_position(index as u64);
        self.bytes.write_all(&data)
    }

    pub(crate) fn get_block_queue(&mut self) -> PeerMessage {
        todo!()
    }
}
