use futures_util::{self, SinkExt};
use tokio::sync::mpsc;

use anyhow::Context;

use crate::messages::PeerMessage;
use crate::messages::payloads::NoPayload;
use crate::messages::payloads::RequestPiecePayload;
use crate::peer::conn::PeerWriter;
use crate::peer::conn::{BoxedMsgStream, PeerState};
use crate::peer_manager::{ReqMessage, ReqMsgFromPeer, ResMessage};

pub mod conn;
mod event_loop;
pub mod handshake;

/// this enum is used to select between different stream-types a peer can receive
#[derive(Debug, PartialEq)]
pub enum Msg {
    /// this will be sent to other peers in order to announce that it has the piece
    ManagerMsg(ResMessage),
    Data(PeerMessage),
    Timeout,
}
pub struct Peer {
    state: PeerState,
    req_queue: Vec<RequestPiecePayload>,
    req_manager_tx: mpsc::Sender<ReqMsgFromPeer>,
    peer_writer: PeerWriter,
}

impl Peer {
    pub fn get_id(&self) -> [u8; 20] {
        self.state.0.peer_id
    }
    async fn send_req_manager(
        &self,
        msg: ReqMessage,
    ) -> Result<(), mpsc::error::SendError<ReqMsgFromPeer>> {
        self.req_manager_tx
            .send(ReqMsgFromPeer {
                peer_id: self.state.0.peer_id,
                msg,
            })
            .await
    }

    /// this sets our interested flag and sends the message to the peer
    async fn set_interested(&mut self, interested: bool) -> anyhow::Result<()> {
        {
            let am_interested = &mut *self.state.0.am_interested.lock().unwrap();
            if interested != *am_interested {
                *am_interested = interested;
            } else {
                // question: is am_interested dropped if I'd just do if (equal) return Ok(())?
                return Ok(());
            }
        }

        let msg = if interested {
            PeerMessage::Interested(NoPayload)
        } else {
            PeerMessage::NotInterested(NoPayload)
        };
        self.peer_writer
            .send(msg)
            .await
            .context("sending interested")?;
        Ok(())
    }

    async fn sever_conn(&mut self) -> bool {
        if *self.state.0.peer_interested.lock().unwrap()
            || *self.state.0.am_interested.lock().unwrap()
        {
            return false;
        }

        self.peer_writer.close().await.is_ok()
    }
}
