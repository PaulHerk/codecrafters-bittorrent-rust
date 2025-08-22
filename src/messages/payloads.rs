use crate::BLOCK_MAX;

pub trait Payload {
    fn from_be_bytes(payload: &[u8]) -> Self;
    fn to_be_bytes(self) -> Vec<u8>;
}

#[derive(Debug, Clone)]
pub struct BitfieldPayload {
    /// a bitfield with each index that downloader has sent set to one and the rest set to zero
    pub pieces_available: Vec<bool>,
}
impl Payload for BitfieldPayload {
    fn from_be_bytes(payload: &[u8]) -> Self {
        let mut pieces_available = vec![false; payload.len() * 8];
        for (byte_i, byte) in payload.iter().enumerate() {
            let mut i = 0_u8;
            while byte << i != 0 && i < 8 {
                pieces_available[byte_i * 8 + i as usize] = true;
                i += 1;
            }
        }
        Self { pieces_available }
    }

    fn to_be_bytes(self) -> Vec<u8> {
        todo!()
    }
}

#[derive(Debug, Clone, Copy, bincode::Encode, bincode::Decode)]
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
}

impl Payload for RequestPiecePayload {
    fn from_be_bytes(payload: &[u8]) -> Self {
        bincode::decode_from_slice(payload, bincode::config::legacy().with_big_endian())
            .unwrap()
            .0
    }
    fn to_be_bytes(self) -> Vec<u8> {
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

impl Payload for ResponsePiecePayload {
    fn from_be_bytes(bytes: &[u8]) -> Self {
        let mut block = [0u8; BLOCK_MAX as usize];
        let block_length = bytes.len() - 8;
        block[..block_length as usize].copy_from_slice(&bytes[8..8 + block_length as usize]);
        Self {
            index: u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            begin: u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            block,
        }
    }
    fn to_be_bytes(self) -> Vec<u8> {
        let mut bytes = vec![0u8; 8 + self.block.len() as usize];
        bytes[0..4].copy_from_slice(&self.index.to_be_bytes());
        bytes[4..8].copy_from_slice(&self.begin.to_be_bytes());
        bytes[8..].copy_from_slice(&self.block);

        bytes
    }
}

#[derive(Debug, Clone, Copy)]
pub struct NoPayload;
impl Payload for NoPayload {
    fn from_be_bytes(_payload: &[u8]) -> Self {
        Self
    }
    fn to_be_bytes(self) -> Vec<u8> {
        vec![]
    }
}
