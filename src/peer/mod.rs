use std::sync::atomic::Ordering;

use futures_util::{self, SinkExt};
use tokio::sync::mpsc;

use crate::messages::PeerMessage;
use crate::messages::payloads::NoPayload;
use crate::peer::conn::PeerWriter;
use crate::peer::conn::send_peer_manager;
use crate::peer::conn::{BoxedMsgStream, PeerState};
use crate::peer::error::PeerError;
use crate::peer_manager::{ReqMessage, ReqMsgFromPeer, ResMessage};

pub mod conn;
mod error;
mod event_loop;
mod extensions;
pub mod initial_handshake;

/// this enum is used to select between different stream-types a peer can receive
#[derive(Debug, PartialEq)]
pub enum Msg {
    /// this will be sent to other peers in order to announce that it has the piece
    Manager(ResMessage),
    Data(PeerMessage),
    Timeout,
}
pub struct Peer {
    pub(crate) state: PeerState,
    queue: ReqQueue,
    peer_manager_tx: mpsc::Sender<ReqMsgFromPeer>,
    peer_writer: PeerWriter,
    // this is an Option because the event-loop takes the Stream and leaves a None in its place while running
    receiver_stream: Option<BoxedMsgStream>,
}
struct ReqQueue {
    to_send: Vec<PeerMessage>,
    have_sent: usize,
}

impl Peer {
    pub fn get_id(&self) -> [u8; 20] {
        self.state.0.peer_id
    }
    async fn send_peer_manager(&self, msg: ReqMessage) -> Result<(), PeerError> {
        let peer_id = self.get_id();
        let msg = ReqMsgFromPeer { peer_id, msg };
        send_peer_manager(&self.peer_manager_tx, msg, peer_id).await
    }
    async fn send_peer(&mut self, msg: PeerMessage) -> Result<(), PeerError> {
        let msg_type_str = msg
            .get_msg_type()
            .map(|msg_type| format!("{msg_type:?}"))
            .unwrap_or("KeepAlive".to_string());
        println!("OUTGOING: {msg:?}");
        self.peer_writer
            .send(msg)
            .await
            .map_err(|error| PeerError::SendToPeer {
                error,
                peer_id: self.get_id(),
                msg_type_str,
            })
    }

    /// this sets our interested flag and sends the message to the peer
    async fn set_interested(&mut self, interested: bool) -> Result<(), PeerError> {
        // checks whether state differs from our,
        // if it does, updates to `interested`
        let prev = self.state.0.am_interested.compare_exchange(
            !interested,
            interested,
            Ordering::Acquire,
            Ordering::Relaxed,
        );
        // if we have actually updated something, send a msg to the peer
        if prev.is_ok() {
            let msg = if interested {
                PeerMessage::Interested(NoPayload)
            } else {
                PeerMessage::NotInterested(NoPayload)
            };
            self.send_peer(msg).await
        } else {
            Ok(())
        }
    }
}

impl ReqQueue {
    fn new() -> Self {
        ReqQueue {
            to_send: Vec::new(),
            have_sent: 0,
        }
    }
}
