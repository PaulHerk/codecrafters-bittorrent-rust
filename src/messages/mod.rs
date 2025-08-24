use bytes::BufMut;
use bytes::{Buf, BytesMut};
use serde_repr::Deserialize_repr;
use tokio_util::codec::Decoder;
use tokio_util::codec::Encoder;

use crate::{
    BitfieldPayload, HavePayload, NoPayload, Payload, RequestPiecePayload, ResponsePiecePayload,
};
pub mod payloads;

#[derive(Debug, Clone)]
pub enum MessageAll {
    Choke(NoPayload),
    Unchoke(NoPayload),
    Interested(NoPayload),
    NotInterested(NoPayload),
    /// TODO: The 'have' message's payload is a single number, the index which that downloader just completed and checked the hash of.
    Have(HavePayload),
    Bitfield(BitfieldPayload),
    Request(RequestPiecePayload),
    Piece(ResponsePiecePayload),
    Cancel(RequestPiecePayload),
    KeepAlive(NoPayload),
}

impl MessageAll {
    pub fn to_be_bytes(&self) -> Vec<u8> {
        match self {
            MessageAll::Choke(payload) => payload.to_be_bytes(),
            MessageAll::Unchoke(payload) => payload.to_be_bytes(),
            MessageAll::Interested(payload) => payload.to_be_bytes(),
            MessageAll::NotInterested(payload) => payload.to_be_bytes(),
            MessageAll::Have(payload) => payload.to_be_bytes(),
            MessageAll::Bitfield(payload) => payload.to_be_bytes(),
            MessageAll::Request(payload) => payload.to_be_bytes(),
            MessageAll::Piece(payload) => payload.to_be_bytes(),
            MessageAll::Cancel(payload) => payload.to_be_bytes(),
            MessageAll::KeepAlive(payload) => payload.to_be_bytes(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize_repr)]
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

#[derive(Debug, Clone)]
pub struct Message {
    /// the length_prefix of the message which includes the message type + payload length
    pub length: u32,
    pub payload: MessageAll,
}

impl Message {
    pub fn new(payload: MessageAll) -> Self {
        Self {
            length: payload.to_be_bytes().len() as u32 + 4, // I don't like that it gets converted to bytes first which gets discarded and when sent, its turned to bytes again
            payload,
        }
    }
    pub fn get_msg_type(&self) -> MessageType {
        match self.payload {
            MessageAll::Choke(_) => MessageType::Choke,
            MessageAll::Unchoke(_) => MessageType::Unchoke,
            MessageAll::Interested(_) => MessageType::Interested,
            MessageAll::NotInterested(_) => MessageType::NotInterested,
            MessageAll::Have(_) => MessageType::Have,
            MessageAll::Bitfield(_) => MessageType::Bitfield,
            MessageAll::Request(_) => MessageType::Request,
            MessageAll::Piece(_) => MessageType::Piece,
            MessageAll::Cancel(_) => MessageType::Cancel,
            MessageAll::KeepAlive(_) => unreachable!(),
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
            return Ok(Some(Message {
                length: 0,
                payload: MessageAll::KeepAlive(NoPayload),
            }));
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

        let data = if length > 1 {
            src[5..4 + length as usize].to_vec()
        } else {
            vec![]
        };
        let data = data.as_slice();
        let msg_type = src[4];
        src.advance(4 + length as usize);

        let payload = match serde_json::from_str::<MessageType>(&msg_type.to_string()) {
            Ok(MessageType::Choke) => Ok(MessageAll::Choke(NoPayload)),
            Ok(MessageType::Unchoke) => Ok(MessageAll::Unchoke(NoPayload)),
            Ok(MessageType::Interested) => Ok(MessageAll::Interested(NoPayload)),
            Ok(MessageType::NotInterested) => Ok(MessageAll::NotInterested(NoPayload)),
            Ok(MessageType::Have) => Ok(MessageAll::Have(HavePayload::from_be_bytes(data))),

            Ok(MessageType::Bitfield) => {
                Ok(MessageAll::Bitfield(BitfieldPayload::from_be_bytes(data)))
            }
            Ok(MessageType::Request) => Ok(MessageAll::Request(
                RequestPiecePayload::from_be_bytes(data),
            )),
            Ok(MessageType::Piece) => {
                Ok(MessageAll::Piece(ResponsePiecePayload::from_be_bytes(data)))
            }
            Ok(MessageType::Cancel) => {
                Ok(MessageAll::Cancel(RequestPiecePayload::from_be_bytes(data)))
            }
            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Invalid message type: {:?}", src[4]),
            )),
        };
        Ok(Some(Message {
            length,
            payload: payload?,
        }))
    }
}

impl Encoder<Message> for MessageFramer {
    type Error = std::io::Error;

    fn encode(&mut self, item: Message, dst: &mut BytesMut) -> Result<(), Self::Error> {
        // Don't send a Message if it is longer than the other end will
        // accept.
        if item.length > MAX {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Frame of length {} is too large.", item.length),
            ));
        }

        // Reserve space in the buffer.
        dst.reserve(item.length as usize);

        // Write the length and string to the buffer.
        dst.extend_from_slice(&item.length.to_be_bytes());
        dst.put_u8(item.get_msg_type() as u8);
        dst.extend_from_slice(item.payload.to_be_bytes().as_slice());
        Ok(())
    }
}
