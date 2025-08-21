use bytes::BufMut;
use bytes::{Buf, BytesMut};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use tokio_util::codec::Decoder;
use tokio_util::codec::Encoder;
pub mod payloads;

#[derive(Debug, Clone, Copy, Serialize_repr, Deserialize_repr, PartialEq)]
#[repr(u8)]
pub enum MessageType {
    Choke = 0,
    Unchoke = 1,
    Interested = 2,
    NotInterested = 3,
    Have = 4,
    Bitfield = 5,
    Request = 6,
    Piece = 7,
    Cancel = 8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub message_length_prefix: u32,
    #[serde(flatten)]
    pub message_id: MessageType,
    pub payload: Vec<u8>,
}

impl Message {
    pub fn new(message_id: MessageType, payload: Vec<u8>) -> Self {
        Self {
            message_length_prefix: payload.len() as u32 + 5,
            message_id,
            payload,
        }
    }
}

pub struct MessageFramer;

const MAX: u32 = 8 * 1024 * 1024;

impl Decoder for MessageFramer {
    type Item = Message;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() < 4 {
            // Not enough data to the read length marker.
            return Ok(None);
        }

        // Read length marker.
        let mut length_bytes = [0u8; 4];
        length_bytes.copy_from_slice(&src[..4]);
        let length = u32::from_be_bytes(length_bytes);

        if length == 0 {
            // this is a keep alive message
            // discard it
            src.advance(4);
            return self.decode(src);
        }

        if src.len() < 5 {
            // Not enough data to read the message type marker.
            return Ok(None);
        }

        // Check that the length is not too large to avoid a denial of
        // service attack where the server runs out of memory.
        if length > MAX {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Frame of length {} is too large.", length),
            ));
        }

        if (src.len() as u32) < 4 + length {
            // The full string has not yet arrived.
            //
            // We reserve more space in the buffer. This is not strictly
            // necessary, but is a good idea performance-wise.
            src.reserve(4 + length as usize - src.len());

            // We inform the Framed that we need more bytes to form the next
            // frame.
            return Ok(None);
        }

        // Use advance to modify src such that it no longer contains
        // this frame.
        let Ok(msg_type): Result<MessageType, _> = serde_json::from_str(&src[4].to_string()) else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid message type: {:?}", src[4]),
            ));
        };
        let data = if length > 1 {
            src[5..4 + length as usize].to_vec()
        } else {
            vec![]
        };
        src.advance(4 + length as usize);

        // Convert the data to a string, or fail if it is not valid utf-8.
        Ok(Some(Message {
            message_length_prefix: length, // this is safe because it was cast from u32 to usize
            message_id: msg_type,
            payload: data,
        }))
    }
}

impl Encoder<Message> for MessageFramer {
    type Error = std::io::Error;

    fn encode(&mut self, item: Message, dst: &mut BytesMut) -> Result<(), Self::Error> {
        // Don't send a Message if it is longer than the other end will
        // accept.
        if item.message_length_prefix > MAX {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "Frame of length {} is too large.",
                    item.message_length_prefix
                ),
            ));
        }

        // Reserve space in the buffer.
        dst.reserve(item.message_length_prefix as usize);

        // Write the length and string to the buffer.
        dst.extend_from_slice(&item.message_length_prefix.to_be_bytes());
        dst.put_u8(item.message_id as u8);
        dst.extend_from_slice(item.payload.as_slice());
        Ok(())
    }
}
