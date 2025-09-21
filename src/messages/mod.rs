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
pub enum PeerMessage {
    Choke(NoPayload),
    Unchoke(NoPayload),
    Interested(NoPayload),
    NotInterested(NoPayload),
    Have(HavePayload),
    Bitfield(BitfieldPayload),
    Request(RequestPiecePayload),
    Piece(ResponsePiecePayload),
    Cancel(RequestPiecePayload),
    KeepAlive(NoPayload),
}

impl PeerMessage {
    pub fn to_be_bytes(&self) -> Vec<u8> {
        match self {
            PeerMessage::Choke(payload) => payload.to_be_bytes(),
            PeerMessage::Unchoke(payload) => payload.to_be_bytes(),
            PeerMessage::Interested(payload) => payload.to_be_bytes(),
            PeerMessage::NotInterested(payload) => payload.to_be_bytes(),
            PeerMessage::Have(payload) => payload.to_be_bytes(),
            PeerMessage::Bitfield(payload) => payload.to_be_bytes(),
            PeerMessage::Request(payload) => payload.to_be_bytes(),
            PeerMessage::Piece(payload) => payload.to_be_bytes(),
            PeerMessage::Cancel(payload) => payload.to_be_bytes(),
            PeerMessage::KeepAlive(payload) => payload.to_be_bytes(),
        }
    }
    fn get_msg_type(&self) -> Option<MessageType> {
        match self {
            PeerMessage::Choke(_) => Some(MessageType::Choke),
            PeerMessage::Unchoke(_) => Some(MessageType::Unchoke),
            PeerMessage::Interested(_) => Some(MessageType::Interested),
            PeerMessage::NotInterested(_) => Some(MessageType::NotInterested),
            PeerMessage::Have(_) => Some(MessageType::Have),
            PeerMessage::Bitfield(_) => Some(MessageType::Bitfield),
            PeerMessage::Request(_) => Some(MessageType::Request),
            PeerMessage::Piece(_) => Some(MessageType::Piece),
            PeerMessage::Cancel(_) => Some(MessageType::Cancel),
            PeerMessage::KeepAlive(_) => None,
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

pub struct MessageFramer;

const MAX: u32 = 8 * 1024 * 1024;

impl Decoder for MessageFramer {
    type Item = PeerMessage;
    type Error = std::io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        if src.len() < 4 {
            // Not enough data to the read length marker.
            return Ok(None);
        }

        // Read length marker.
        let mut length_bytes = [0u8; 4];
        length_bytes.copy_from_slice(&src[..4]);
        // length without the length prefix
        let data_length = u32::from_be_bytes(length_bytes);

        if data_length == 0 {
            // this is a keep alive message
            // discard it
            src.advance(4);
            return Ok(Some(PeerMessage::KeepAlive(NoPayload)));
        }

        if src.len() < 5 {
            // Not enough data to read the message type marker.
            return Ok(None);
        }

        // Check that the length is not too large to avoid a denial of
        // service attack where the server runs out of memory.
        if data_length > MAX {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Frame of length {data_length} is too large"),
            ));
        }

        if (src.len() as u32) < 4 + data_length {
            // The full string has not yet arrived.
            //
            // We reserve more space in the buffer. This is not strictly
            // necessary, but is a good idea performance-wise.
            src.reserve(4 + data_length as usize - src.len());

            // We inform the Framed that we need more bytes to form the next
            // frame.
            return Ok(None);
        }

        let data = if data_length > 1 {
            src[5..4 + data_length as usize].to_vec()
        } else {
            vec![]
        };
        let data = data.as_slice();
        let msg_type = src[4];
        src.advance(4 + data_length as usize);

        let payload = match serde_json::from_str::<MessageType>(&msg_type.to_string()) {
            Ok(MessageType::Choke) => Ok::<_, std::io::Error>(PeerMessage::Choke(NoPayload)),
            Ok(MessageType::Unchoke) => Ok(PeerMessage::Unchoke(NoPayload)),
            Ok(MessageType::Interested) => Ok(PeerMessage::Interested(NoPayload)),
            Ok(MessageType::NotInterested) => Ok(PeerMessage::NotInterested(NoPayload)),
            Ok(MessageType::Have) => Ok(PeerMessage::Have(HavePayload::from_be_bytes(data))),

            Ok(MessageType::Bitfield) => {
                Ok(PeerMessage::Bitfield(BitfieldPayload::from_be_bytes(data)))
            }
            Ok(MessageType::Request) => Ok(PeerMessage::Request(
                RequestPiecePayload::from_be_bytes(data),
            )),
            Ok(MessageType::Piece) => Ok(PeerMessage::Piece(ResponsePiecePayload::from_be_bytes(
                data,
            ))),
            Ok(MessageType::Cancel) => Ok(PeerMessage::Cancel(RequestPiecePayload::from_be_bytes(
                data,
            ))),
            _ => {
                // now theoretically we would panic here or some sort.
                // however there are extensions with make use of different message types.
                // we will ignore them tho
                // Err(std::io::Error::new(
                //     std::io::ErrorKind::InvalidData,
                //     format!("Invalid message type: {:?}", src[4]),
                // ))
                return Ok(None);
            }
        };
        Ok(Some(payload?))
    }
}

impl Encoder<PeerMessage> for MessageFramer {
    type Error = std::io::Error;

    fn encode(&mut self, item: PeerMessage, dst: &mut BytesMut) -> Result<(), Self::Error> {
        // Don't send a Message if it is longer than the other end will
        // accept.
        let bytes = item.to_be_bytes();
        // if it's a keep-alive message, it has a length of 0 but is discarded afterwards
        let length = bytes.len() + 4;
        if length as u32 > MAX {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Frame of length {} is too large.", length),
            ));
        }

        let Some(msg_type) = item.get_msg_type() else {
            // it's a keep alive message
            return Ok(());
        };

        // Reserve space in the buffer.
        dst.reserve(length);

        // Write the length and string to the buffer.
        dst.extend_from_slice(&length.to_be_bytes());
        dst.put_u8(msg_type as u8);
        dst.extend_from_slice(&bytes);

        dbg!(&dst);
        Ok(())
    }
}
