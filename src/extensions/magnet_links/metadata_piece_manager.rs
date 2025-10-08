/// This is all for the PeerManager
use std::io::{self, Cursor, Write};

use sha1::{Digest, Sha1};

use crate::{
    magnet_links::metadata_requester::{MetadataMsg, MetadataMsgType},
    torrent::{InfoHash, Metainfo},
};

/// The metadata is handled in blocks of 16KiB (16384 Bytes).
const METADATA_BLOCK_SIZE: usize = 16384;

#[derive(Debug)]
pub(crate) struct MetadataPieceManager {
    queue: Vec<bool>,
    bytes: Cursor<Vec<u8>>,
    pub info_hash: InfoHash,
}

impl MetadataPieceManager {
    pub(crate) fn new(info_hash: InfoHash) -> Self {
        Self {
            queue: Vec::new(),
            bytes: Cursor::new(Vec::new()),
            info_hash,
        }
    }

    pub(crate) fn add_block(&mut self, index: u32, data: Vec<u8>) -> io::Result<()> {
        self.bytes.set_position(index as u64);
        self.bytes.write_all(&data)?;
        self.queue[index as usize] = true;
        Ok(())
    }

    /// returns Ok(None) if we're finished downloading the Metadata
    /// returns Err(..) if it couldn't serialize the MetadataMsg to bytes
    /// returns Ok(Vec<u8>) of the data bytes in the BasicExtensionPayload
    pub(crate) fn get_block_req_data(&mut self) -> Result<Option<Vec<u8>>, serde_bencode::Error> {
        let Some(piece_index) = self.queue.iter().position(|i_have| !*i_have) else {
            return Ok(None);
        };
        let msg = MetadataMsg {
            msg_type: MetadataMsgType::Request,
            piece_index: piece_index as u32,
            total_size: None,
        };
        Ok(Some(serde_bencode::to_bytes(&msg)?))
    }

    /// # This method clears the MetadataPieceManager
    pub(crate) fn clear_and_set_len(&mut self, length: usize) {
        let n_blocks = length.div_ceil(METADATA_BLOCK_SIZE);
        self.queue = vec![false; n_blocks];
        self.bytes = Cursor::new(Vec::with_capacity(length));
    }

    pub(crate) fn check_finished(&mut self) -> bool {
        if self.queue.iter().all(|i| *i) {
            let mut hasher = Sha1::new();
            hasher.update(self.bytes.get_ref());
            let sha1: [u8; 20] = hasher.finalize().into();

            if sha1 == self.info_hash.0 {
                return true;
            } else {
                self.queue = vec![false; self.queue.len()];
            }
        }
        false
    }

    pub(crate) fn get_metadata(&self) -> Result<Metainfo, serde_bencode::Error> {
        serde_bencode::from_bytes(self.bytes.get_ref())
    }
}
