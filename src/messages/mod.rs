use std::marker::PhantomData;
use std::ops::Deref;

use bytes::BufMut;
use bytes::{Buf, BytesMut};
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};
use tokio_util::codec::Decoder;
use tokio_util::codec::Encoder;

use crate::{BitfieldPayload, NoPayload, Payload, RequestPiecePayload, ResponsePiecePayload};
pub mod payloads;

#[derive(Debug, Clone)]
pub enum MessageAll {
    Choke(P<NoPayload>),
    Unchoke(P<NoPayload>),
    Interested(P<NoPayload>),
    NotInterested(P<NoPayload>),
    /// TODO: The 'have' message's payload is a single number, the index which that downloader just completed and checked the hash of.
    Have(P<NoPayload>),
    Bitfield(P<BitfieldPayload>),
    Request(P<RequestPiecePayload>),
    Piece(P<ResponsePiecePayload>),
    Cancel(P<RequestPiecePayload>),
    KeepAlive(P<NoPayload>),
}

impl MessageAll {
    pub fn to_be_bytes(&self) -> Vec<u8> {
        match self {
            MessageAll::Choke(payload) => payload.0.to_be_bytes(),
            MessageAll::Unchoke(payload) => payload.0.to_be_bytes(),
            MessageAll::Interested(payload) => payload.0.to_be_bytes(),
            MessageAll::NotInterested(payload) => payload.0.to_be_bytes(),
            MessageAll::Have(payload) => payload.0.to_be_bytes(),
            MessageAll::Bitfield(payload) => payload.clone().0.to_be_bytes(),
            MessageAll::Request(payload) => payload.0.to_be_bytes(),
            MessageAll::Piece(payload) => payload.0.to_be_bytes(),
            MessageAll::Cancel(payload) => payload.0.to_be_bytes(),
            MessageAll::KeepAlive(payload) => payload.0.to_be_bytes(),
        }
    }
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
    pub fn get_msg_type(&self) -> u8 {
        match self.payload {
            MessageAll::Choke(_) => 0,
            MessageAll::Unchoke(_) => 1,
            MessageAll::Interested(_) => 2,
            MessageAll::NotInterested(_) => 3,
            MessageAll::Have(_) => 4,
            MessageAll::Bitfield(_) => 5,
            MessageAll::Request(_) => 6,
            MessageAll::Piece(_) => 7,
            MessageAll::Cancel(_) => 8,
            MessageAll::KeepAlive(_) => unreachable!(),
        }
    }
}

/// A wrapper for payloads that implements `Payload` for `MessageAll`.
#[derive(Debug, Clone)]
pub struct P<T: Payload>(pub T);

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
                payload: MessageAll::KeepAlive(P(NoPayload)),
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

        let payload = match msg_type {
            0 => Ok(MessageAll::Choke(P(NoPayload))),
            1 => Ok(MessageAll::Unchoke(P(NoPayload))),
            2 => Ok(MessageAll::Interested(P(NoPayload))),
            3 => Ok(MessageAll::NotInterested(P(NoPayload))),
            4 => Ok(MessageAll::Have(P(NoPayload))),
            5 => Ok(MessageAll::Bitfield(P(BitfieldPayload::from_be_bytes(
                data,
            )))),
            6 => Ok(MessageAll::Request(P(RequestPiecePayload::from_be_bytes(
                data,
            )))),
            7 => Ok(MessageAll::Piece(P(ResponsePiecePayload::from_be_bytes(
                data,
            )))),
            8 => Ok(MessageAll::Cancel(P(RequestPiecePayload::from_be_bytes(
                data,
            )))),
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
        dst.put_u8(item.get_msg_type());
        dst.extend_from_slice(item.payload.to_be_bytes().as_slice());
        Ok(())
    }
}
