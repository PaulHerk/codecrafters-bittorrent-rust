use crate::BLOCK_MAX;

#[derive(Debug, Clone)]
pub struct BitfieldPayload {
    /// a bitfield with each index that downloader has sent set to one and the rest set to zero
    pub pieces_available: u8,
}
impl BitfieldPayload {
    pub fn from_be_bytes(payload: &[u8]) -> Self {
        let mut pieces_available = 0;
        for byte in payload {
            let mut i = 0_u8;
            while byte << i != 0 && i < 8 {
                pieces_available += 1;
                i += 1;
            }
        }
        Self { pieces_available }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RequestPiecePayload {
    pub index: u32,
    pub begin: u32,
    pub length: u32,
}

impl RequestPiecePayload {
    pub fn new(index: u32, begin: u32, length: u32) -> Self {
        Self {
            index,
            begin,
            length,
        }
    }
    pub fn to_be_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![0u8; 12];
        bytes[0..4].copy_from_slice(&self.index.to_be_bytes());
        bytes[4..8].copy_from_slice(&self.begin.to_be_bytes());
        bytes[8..12].copy_from_slice(&self.length.to_be_bytes());
        bytes
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ResponsePiecePayload {
    pub index: u32,
    pub begin: u32,
    pub block: [u8; BLOCK_MAX as usize],
}

impl ResponsePiecePayload {
    pub fn from_be_bytes(bytes: &[u8], block_length: u32) -> Self {
        let mut block = [0u8; BLOCK_MAX as usize];
        block[..block_length as usize].copy_from_slice(&bytes[8..8 + block_length as usize]);
        Self {
            index: u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            begin: u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            block,
        }
    }
}
