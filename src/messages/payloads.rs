pub trait Payload {
    fn from_be_bytes(payload: &[u8]) -> Self;
    /// the bytes in BE order without the length & message prefix
    fn to_be_bytes(&self) -> Vec<u8>;
}

#[derive(Debug, Clone, PartialEq)]
pub struct BitfieldPayload {
    /// a bitfield with each index that downloader has sent set to one and the rest set to zero
    pub pieces_available: Vec<bool>,
}
impl BitfieldPayload {
    pub(crate) fn is_nothing(&self) -> bool {
        self.pieces_available.iter().all(|b| !*b)
    }
}
impl Payload for BitfieldPayload {
    fn from_be_bytes(payload: &[u8]) -> Self {
        let mut pieces_available = vec![false; payload.len() * 8];
        for (byte_i, byte) in payload.iter().enumerate() {
            let mut i = 0_u8;
            while i < 8 && byte << i != 0 {
                pieces_available[byte_i * 8 + i as usize] = true;
                i += 1;
            }
        }
        Self { pieces_available }
    }

    fn to_be_bytes(&self) -> Vec<u8> {
        self.pieces_available
            .chunks(8)
            .map(|byte| {
                byte.iter()
                    .enumerate()
                    .fold(0_u8, |acc, (i, &b)| acc | (if b { 128_u8 >> i } else { 0 }))
            })
            .collect()
    }
}

#[derive(Debug, Clone, Copy, bincode::Encode, bincode::Decode, PartialEq)]
pub struct RequestPiecePayload {
    /// the index of the piece
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
    fn to_be_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![0u8; 12];
        bytes[0..4].copy_from_slice(&self.index.to_be_bytes());
        bytes[4..8].copy_from_slice(&self.begin.to_be_bytes());
        bytes[8..12].copy_from_slice(&self.length.to_be_bytes());
        bytes
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResponsePiecePayload {
    pub index: u32,
    pub begin: u32,
    pub block: Vec<u8>,
}

impl Payload for ResponsePiecePayload {
    fn from_be_bytes(bytes: &[u8]) -> Self {
        let block_length = bytes.len() - 8;
        let mut block = vec![0u8; block_length];
        block[..block_length].copy_from_slice(&bytes[8..8 + block_length]);
        Self {
            index: u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            begin: u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            block: block.to_vec(),
        }
    }
    fn to_be_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![0u8; 8 + self.block.len()];
        bytes[0..4].copy_from_slice(&self.index.to_be_bytes());
        bytes[4..8].copy_from_slice(&self.begin.to_be_bytes());
        bytes[8..].copy_from_slice(&self.block);

        bytes
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HavePayload {
    /// a single number, the index which that downloader just completed and checked the hash of
    pub piece_index: u32,
}

impl Payload for HavePayload {
    fn from_be_bytes(payload: &[u8]) -> Self {
        HavePayload {
            piece_index: u32::from_be_bytes(
                payload[0..4]
                    .try_into()
                    .expect("The have payload is apparently not 4 bytes…"),
            ),
        }
    }
    fn to_be_bytes(&self) -> Vec<u8> {
        let mut bytes = vec![0u8; 4];
        bytes[0..4].copy_from_slice(&self.piece_index.to_be_bytes());
        bytes
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoPayload;
impl Payload for NoPayload {
    fn from_be_bytes(_payload: &[u8]) -> Self {
        Self
    }
    fn to_be_bytes(&self) -> Vec<u8> {
        vec![]
    }
}
