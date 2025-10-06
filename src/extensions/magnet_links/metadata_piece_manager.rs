use crate::torrent::InfoHash;

/// The metadata is handled in blocks of 16KiB (16384 Bytes).
const METADATA_BLOCK_SIZE: usize = 16384;

pub(crate) struct MetadataPieceManager {
    queue: Vec<bool>,
    bytes: Vec<u8>,
    info_hash: InfoHash,
}

impl MetadataPieceManager {
    pub(crate) fn new(info_hash: InfoHash) -> Self {
        Self {
            queue: Vec::new(),
            bytes: Vec::with_capacity(METADATA_BLOCK_SIZE * 10), // let's just allocate enough space
            info_hash,
        }
    }
}
