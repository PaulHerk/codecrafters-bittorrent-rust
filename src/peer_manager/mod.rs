use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    path::PathBuf,
};

use tokio::sync::mpsc;

use crate::{
    Torrent,
    database::DBConnection,
    messages::payloads::{BitfieldPayload, RequestPiecePayload, ResponsePiecePayload},
    peer::conn::PeerState,
    peer_manager::error::PeerManagerError,
};

mod error;
mod file_manager;
mod req_preparer;

pub const BLOCK_QUEUE_SIZE_MAX: usize = 20;
/// how many pieces are in the queue at max
const MAX_PIECES_IN_PARALLEL: usize = 5;

/// A message sent by a local peer to this Manager
#[derive(Debug, Clone)]
pub enum ReqMessage {
    NewConnection(PeerConn),
    NeedBlockQueue,
    GotBlock(ResponsePiecePayload),
    NeedBlock(RequestPiecePayload),
    WhatDoWeHave,
}

pub struct ReqMsgFromPeer {
    pub(crate) peer_id: [u8; 20],
    pub(crate) msg: ReqMessage,
}

// a peer announces to us that he exists via the mpsc
// We create peer with our current have bitfield which he can send to new connections and we send

// Next-up:
// - split it into ReqMessage and ResMessage (TODO: better naming)
// - the PeerManager has access to all peers with a HashMap<PeerHash, mpsc::Sender<ResMessage>>, and maybe also the download speed
// - allows to implement:
//  - rarest-first-piece-selection
//  - peer not shutting down if the queue is empty, rather the Manager sends the shuttdown to all peers
//    (the peers have to send the shutdown back)
//  - remove the has-broadcaster from peers, Manager handles this
//  - choking: 4 active downloaders

#[derive(Debug, Clone, PartialEq)]
pub enum ResMessage {
    NewBlockQueue(Vec<RequestPiecePayload>),
    Block(Option<ResponsePiecePayload>),
    WeHave(BitfieldPayload),
    FinishedPiece(u32),
    FinishedFile,
}

pub struct PeerManager {
    /// the output file
    file: File,
    db_conn: DBConnection,
    rx: mpsc::Receiver<ReqMsgFromPeer>,
    /// I need this information too often to always query the DB
    /// so let's cache it
    have: Vec<bool>,
    /// if it's None, we are finished
    download_queue: Option<Vec<PieceState>>,
    pub torrent: Torrent,
    /// this is also cached, it'll never change
    info_hash: String,
    peers: HashMap<[u8; 20], PeerConn>,
}

#[derive(Debug, Clone)]
pub struct PeerConn {
    pub(super) sender: mpsc::Sender<ResMessage>,
    pub(super) identifier: PeerState,
}

#[derive(Clone, Debug)]
struct PieceState {
    blocks: Vec<BlockState>,
    piece_i: u32,
    buf: Vec<u8>,
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
enum BlockState {
    Finished,
    InProcess,
    None,
}

impl BlockState {
    pub(self) fn is_finished(&self) -> bool {
        *self == BlockState::Finished
    }
    pub(self) fn is_none(&self) -> bool {
        *self == BlockState::None
    }
}

impl PeerManager {
    pub async fn init(
        rx: mpsc::Receiver<ReqMsgFromPeer>,
        file_path: Option<PathBuf>,
        torrent_path: PathBuf,
    ) -> Result<Self, PeerManagerError> {
        let db_conn = DBConnection::new().await?;
        let torrent = Torrent::read_from_file(&torrent_path)?;
        let file_info = db_conn
            .set_and_get_file(file_path, torrent_path, &torrent)
            .await?;

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .truncate(false)
            .open(&file_info.file)
            .map_err(|error| PeerManagerError::OpenError {
                path: file_info.file.to_path_buf(),
                error,
            })?;

        let download_state = if file_info.is_finished() {
            None
        } else {
            Some(Vec::with_capacity(MAX_PIECES_IN_PARALLEL))
        };
        let info_hash = hex::encode(torrent.info_hash());

        Ok(Self {
            file,
            db_conn,
            rx,
            have: file_info.bitfield.to_vec(),
            download_queue: download_state,
            torrent,
            info_hash,
            peers: HashMap::new(),
        })
    }

    pub async fn run(mut self) -> anyhow::Result<()> {
        while let Some(peer_msg) = self.rx.recv().await {
            match peer_msg.msg {
                ReqMessage::NewConnection(peer_conn) => {
                    self.peers.insert(peer_msg.peer_id, peer_conn);
                }
                ReqMessage::GotBlock(block) => {
                    if let Some(piece_index) = self.write_block(block).await? {
                        let msg = ResMessage::FinishedPiece(piece_index);
                        self.send_peer(&peer_msg.peer_id, msg).await?;
                    };
                }
                ReqMessage::NeedBlock(block) => {
                    if self.have[block.index as usize] {
                        let block = self.get_block(block);
                        let msg = ResMessage::Block(block);
                        self.send_peer(&peer_msg.peer_id, msg).await?;
                    } else {
                        todo!(
                            "send a message to the peer that we don't have the block? (not sure if there's a message type for this)"
                        );
                    }
                }
                ReqMessage::NeedBlockQueue => {
                    let peer_has = self.get_peer_has(&peer_msg.peer_id)?;
                    let blocks = self.prepare_next_blocks(BLOCK_QUEUE_SIZE_MAX, peer_has);
                    let msg = ResMessage::NewBlockQueue(blocks);
                    self.send_peer(&peer_msg.peer_id, msg).await?;
                }
                ReqMessage::WhatDoWeHave => {
                    let msg = ResMessage::WeHave(BitfieldPayload {
                        pieces_available: self.have.clone(),
                    });
                    self.send_peer(&peer_msg.peer_id, msg).await?;
                }
            }
        }

        Ok(())
    }

    async fn send_peer(
        &mut self,
        peer_id: &[u8; 20],
        msg: ResMessage,
    ) -> Result<(), PeerManagerError> {
        let peer = self
            .peers
            .get_mut(peer_id)
            .ok_or(PeerManagerError::PeerNotFound)?;
        peer.sender
            .send(msg)
            .await
            .map_err(|error| PeerManagerError::SendError {
                peer_id: *peer_id,
                error,
                msg: "Sending a message.".to_string(),
            })
    }

    fn get_peer_has(&self, peer_id: &[u8; 20]) -> Result<Vec<bool>, PeerManagerError> {
        Ok(self
            .peers
            .get(peer_id)
            .ok_or(PeerManagerError::PeerNotFound)?
            .identifier
            .0
            .has
            .lock()
            .unwrap()
            .clone())
    }
}
