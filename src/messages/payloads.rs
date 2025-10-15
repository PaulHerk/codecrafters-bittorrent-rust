use bytes::{Bytes, BytesMut};

pub trait Payload {
    fn from_be_bytes(payload: &[u8]) -> Self;
    /// the bytes in BE order without the length & message prefix
    fn to_be_bytes(&self) -> Bytes;
}

#[derive(Debug, Clone, PartialEq)]
pub struct BitfieldPayload {
    /// a bitfield with each index that downloader has sent set to one and the rest set to zero
    pub(crate) pieces_available: Vec<bool>,
}
impl BitfieldPayload {
    pub(crate) fn is_empty(&self) -> bool {
        self.pieces_available.iter().all(|b| !*b)
    }
    pub(crate) fn is_finished(&self) -> bool {
        self.pieces_available.iter().all(|b| *b)
    }
}
impl Payload for BitfieldPayload {
    fn from_be_bytes(payload: &[u8]) -> Self {
        let mut pieces_available = vec![false; payload.len() * 8];
        for (byte_i, byte) in payload.iter().enumerate() {
            for i in 0..8 {
                if 1u8.rotate_right(i + 1) & byte != 0 {
                    pieces_available[byte_i * 8 + i as usize] = true;
                }
            }
        }
        Self { pieces_available }
    }

    fn to_be_bytes(&self) -> Bytes {
        Bytes::from_iter(self.pieces_available.chunks(8).map(|byte| {
            byte.iter()
                .enumerate()
                .fold(0_u8, |acc, (i, &b)| acc | (if b { 128_u8 >> i } else { 0 }))
        }))
    }
}

#[derive(Debug, Clone, Copy, bincode::Encode, bincode::Decode, PartialEq)]
pub struct RequestPiecePayload {
    /// the index of the piece
    pub(crate) index: u32,
    pub(crate) begin: u32,
    pub(crate) length: u32,
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
    fn to_be_bytes(&self) -> Bytes {
        let mut bytes = BytesMut::zeroed(12);
        bytes[0..4].copy_from_slice(&self.index.to_be_bytes());
        bytes[4..8].copy_from_slice(&self.begin.to_be_bytes());
        bytes[8..12].copy_from_slice(&self.length.to_be_bytes());
        bytes.freeze()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResponsePiecePayload {
    pub(crate) index: u32,
    pub(crate) begin: u32,
    pub(crate) block: Bytes,
}

impl Payload for ResponsePiecePayload {
    fn from_be_bytes(bytes: &[u8]) -> Self {
        let block_length = bytes.len() - 8;
        let mut block = BytesMut::zeroed(block_length);
        block[..block_length].copy_from_slice(&bytes[8..8 + block_length]);
        Self {
            index: u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
            begin: u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
            block: block.freeze(),
        }
    }
    fn to_be_bytes(&self) -> Bytes {
        let mut bytes = BytesMut::zeroed(8 + self.block.len());
        bytes[0..4].copy_from_slice(&self.index.to_be_bytes());
        bytes[4..8].copy_from_slice(&self.begin.to_be_bytes());
        bytes[8..].copy_from_slice(&self.block);

        bytes.freeze()
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
    fn to_be_bytes(&self) -> Bytes {
        Bytes::from_owner(self.piece_index.to_be_bytes())
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoPayload;
impl Payload for NoPayload {
    fn from_be_bytes(_payload: &[u8]) -> Self {
        Self
    }
    fn to_be_bytes(&self) -> Bytes {
        Bytes::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bitfield_payload() {
        let payload = BitfieldPayload {
            pieces_available: vec![true, false, true, false, true, false, true, false, true],
        };
        let bytes = payload.to_be_bytes();
        let payload2 = BitfieldPayload::from_be_bytes(&bytes);
        // The from_be_bytes function pads with 'false' values to the next full byte,
        // so we need to do the same for the original payload to compare them.
        let mut expected_pieces = payload.pieces_available.clone();
        expected_pieces.resize(16, false);
        assert_eq!(payload2.pieces_available, expected_pieces);
    }

    #[test]
    fn test_request_piece_payload() {
        let payload = RequestPiecePayload {
            index: 1,
            begin: 2,
            length: 3,
        };
        let bytes = payload.to_be_bytes();
        let payload2 = RequestPiecePayload::from_be_bytes(&bytes);
        assert_eq!(payload, payload2);
    }

    #[test]
    fn test_response_piece_payload() {
        let payload = ResponsePiecePayload {
            index: 1,
            begin: 2,
            block: Bytes::from_owner([1, 2, 3, 4]),
        };
        let bytes = payload.to_be_bytes();
        let payload2 = ResponsePiecePayload::from_be_bytes(&bytes);
        assert_eq!(payload, payload2);
    }

    #[test]
    fn test_have_payload() {
        let payload = HavePayload { piece_index: 1 };
        let bytes = payload.to_be_bytes();
        let payload2 = HavePayload::from_be_bytes(&bytes);
        assert_eq!(payload, payload2);
    }
}
