//! This is all for the PeerManager

use bytes::{Bytes, BytesMut};
use sha1::{Digest, Sha1};

use crate::{
    magnet_links::metadata_msg::{MetadataMsg, MetadataMsgType},
    peer_manager::BlockState,
    torrent::{InfoHash, Metainfo},
};

/// The metadata is handled in blocks of 16KiB (16384 Bytes).
const METADATA_BLOCK_SIZE: usize = 1 << 14;

#[derive(Debug)]
pub(crate) struct MetadataPieceManager {
    queue: Vec<BlockState>,
    bytes: BytesMut,
    pub info_hash: InfoHash,
}

impl MetadataPieceManager {
    pub(crate) fn new(info_hash: InfoHash) -> Self {
        Self {
            queue: Vec::new(),
            bytes: BytesMut::new(),
            info_hash,
        }
    }

    pub(crate) fn add_block(&mut self, index: u32, data: Bytes) {
        let len = data.len();
        let begin = index as usize * METADATA_BLOCK_SIZE;
        self.bytes[begin..begin + len].copy_from_slice(&data);
        self.queue[index as usize] = BlockState::Finished;
    }

    /// returns Ok(None) if we're finished downloading the Metadata
    /// returns Err(..) if it couldn't serialize the MetadataMsg to bytes
    /// returns Ok(Bytes) of the data bytes in the BasicExtensionPayload
    pub(crate) fn get_block_req_data(&mut self) -> Result<Option<Bytes>, serde_bencode::Error> {
        let Some(piece_index) = self
            .queue
            .iter()
            .position(|i_have| *i_have == BlockState::None)
        // .or_else(|| {
        //     self.queue
        //         .iter()
        //         .enumerate()
        //         .filter_map(|(index, i_have)| (*i_have == BlockState::InProcess).then(|| index))
        //         .choose(&mut rand::rng())
        // })
        // TODO: send the message to multiple peers (if I'd just do that, it would send the same one multiple times to the same peer)
        else {
            return Ok(None);
        };
        dbg!(piece_index);
        self.queue[piece_index] = BlockState::InProcess;
        let msg = MetadataMsg {
            msg_type: MetadataMsgType::Request,
            piece_index: piece_index as u32,
            total_size: None,
        };
        Ok(Some(serde_bencode::to_bytes(&msg)?.into()))
    }

    /// initializes the fields of the MetadataPieceManager (like which blocks are finished)
    /// with the given length
    pub(crate) fn set_len(&mut self, length: usize) {
        if !self.queue.is_empty() {
            // it's already initialized
            return;
        }
        let n_blocks = length.div_ceil(METADATA_BLOCK_SIZE);
        self.queue = vec![BlockState::None; n_blocks];
        self.bytes = BytesMut::zeroed(length);
    }

    pub(crate) fn check_finished(&mut self) -> bool {
        if self.queue.iter().all(|i| *i == BlockState::Finished) {
            let mut hasher = Sha1::new();
            hasher.update(&self.bytes);
            let sha1: [u8; 20] = hasher.finalize().into();

            if sha1 == self.info_hash.0 {
                return true;
            } else {
                self.queue = vec![BlockState::None; self.queue.len()];
            }
        }
        false
    }

    pub(crate) fn get_metadata(&self) -> Result<Metainfo, serde_bencode::Error> {
        serde_bencode::from_bytes(&self.bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::torrent::InfoHash;

    #[test]
    fn test_new_metadata_piece_manager() {
        let info_hash = InfoHash([0x00; 20]);
        let manager = MetadataPieceManager::new(info_hash);
        assert!(manager.queue.is_empty());
        assert_eq!(manager.bytes.len(), 0);
        assert_eq!(manager.info_hash, info_hash);
    }

    #[test]
    fn test_set_len() {
        let info_hash = InfoHash([0x00; 20]);
        let mut manager = MetadataPieceManager::new(info_hash);

        // Test initialization
        manager.set_len(METADATA_BLOCK_SIZE * 3 + 100); // 4 blocks
        assert_eq!(manager.queue.len(), 4);
        assert!(manager.queue.iter().all(|&b| b == BlockState::None));
        assert_eq!(manager.bytes.len(), METADATA_BLOCK_SIZE * 3 + 100);
        assert!(manager.bytes.iter().all(|b| *b == 0));

        // Test that calling set_len again does nothing if already initialized
        manager.queue[0] = BlockState::Finished; // Mark one piece as received
        manager.set_len(100);
        assert_eq!(manager.queue.len(), 4); // Should still be 4, not 1
        assert_eq!(manager.queue[0], BlockState::Finished); // Should still be true
    }

    #[test]
    fn test_add_block() {
        let info_hash = InfoHash([0x00; 20]);
        let mut manager = MetadataPieceManager::new(info_hash);
        manager.set_len(METADATA_BLOCK_SIZE * 2 + 3); // 3 blocks

        let block_data_0 = Bytes::from_owner([0x01; METADATA_BLOCK_SIZE]);
        let block_data_1 = Bytes::from_owner([0x02; METADATA_BLOCK_SIZE]);

        manager.add_block(0, block_data_0.clone());
        assert_eq!(manager.queue[0], BlockState::Finished);
        assert_eq!(&manager.bytes[0..METADATA_BLOCK_SIZE], block_data_0);

        manager.add_block(1, block_data_1.clone());
        assert_eq!(manager.queue[1], BlockState::Finished);
        assert_eq!(
            &manager.bytes[METADATA_BLOCK_SIZE..METADATA_BLOCK_SIZE * 2],
            block_data_1
        );

        manager.add_block(2, Bytes::from_owner([1, 2, 3]));
        assert_eq!(manager.queue[2], BlockState::Finished);
        assert_eq!(
            &manager.bytes[METADATA_BLOCK_SIZE * 2..METADATA_BLOCK_SIZE * 2 + 3],
            &[1, 2, 3]
        );
    }

    #[test]
    fn test_get_block_req_data() {
        let info_hash = InfoHash([0x00; 20]);
        let mut manager = MetadataPieceManager::new(info_hash);
        manager.set_len(METADATA_BLOCK_SIZE * 3); // 3 blocks

        // Request first block
        let req_data_0 = manager.get_block_req_data().unwrap().unwrap();
        assert_eq!(req_data_0, b"d8:msg_typei0e5:piecei0ee".to_vec());
        let msg_0: MetadataMsg = serde_bencode::from_bytes(&req_data_0).unwrap();
        assert_eq!(msg_0.msg_type, MetadataMsgType::Request);
        assert_eq!(msg_0.piece_index, 0);
        assert_eq!(msg_0.total_size, None);
        assert_eq!(manager.queue[0], BlockState::InProcess);

        // Request second block
        let req_data_1 = manager.get_block_req_data().unwrap().unwrap();
        assert_eq!(req_data_1, b"d8:msg_typei0e5:piecei1ee".to_vec());
        let msg_1: MetadataMsg = serde_bencode::from_bytes(&req_data_1).unwrap();
        assert_eq!(msg_1.msg_type, MetadataMsgType::Request);
        assert_eq!(msg_1.piece_index, 1);

        manager.queue[0] = BlockState::Finished;
        manager.queue[1] = BlockState::Finished;
        manager.queue[2] = BlockState::Finished;

        // Should return None when all blocks are received
        assert!(manager.get_block_req_data().unwrap().is_none());
    }

    #[test]
    fn test_check_finished() {
        let metadata_bytes = BytesMut::from(&[0x01; METADATA_BLOCK_SIZE][..]);
        let mut hasher = Sha1::new();
        hasher.update(&metadata_bytes);
        let info_hash = InfoHash(hasher.finalize().into());
        let mut manager = MetadataPieceManager::new(info_hash);
        manager.set_len(METADATA_BLOCK_SIZE);

        manager.bytes = metadata_bytes;
        manager.queue = vec![BlockState::Finished; 1];

        assert!(manager.check_finished());

        // simulate not matching metainfo
        manager.bytes = BytesMut::from(&[0x00; METADATA_BLOCK_SIZE][..]);
        assert!(!manager.check_finished());
        // Queue should be reset
        assert!(manager.queue.iter().all(|&b| b == BlockState::None));
    }

    #[test]
    fn test_check_finished_not_all_pieces() {
        let info_hash = InfoHash([0x00; 20]);
        let mut manager = MetadataPieceManager::new(info_hash);
        manager.set_len(METADATA_BLOCK_SIZE * 2);

        let bytes = vec![0x00; METADATA_BLOCK_SIZE];
        manager.bytes = BytesMut::from(&[0x00; METADATA_BLOCK_SIZE][..]);
        manager.queue[0] = BlockState::Finished; // Only one piece received
        assert!(!manager.check_finished());
        // queue should still be the same
        assert_eq!(*manager.bytes, bytes);
    }

    #[test]
    fn test_get_metadata() {
        // let info_hash = InfoHash([0x00; 20]);
        // let mut manager = MetadataPieceManager::new(info_hash);
        // manager.set_len(10);

        // let bencoded_metainfo = b"4:infod4:name12:a test file12:piece lengthi32768e6e";
        // manager.bytes = Cursor::new(bencoded_metainfo.to_vec());

        // let metainfo = manager.get_metadata().unwrap();
        // assert_eq!(metainfo.name, "a test file");
        // assert_eq!(metainfo.piece_length, 32768);
    }
}
