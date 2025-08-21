use crate::BLOCK_MAX;

#[derive(Debug, Clone, Copy)]
pub struct RequestPieceMsgPayload {
    pub index: u32,
    pub begin: u32,
    pub length: u32,
}

impl RequestPieceMsgPayload {
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
pub struct ResponsePieceMsgPayload {
    pub index: u32,
    pub begin: u32,
    pub block: [u8; BLOCK_MAX as usize],
}

impl ResponsePieceMsgPayload {
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
