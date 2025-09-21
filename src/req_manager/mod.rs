use std::{
    collections::{HashMap, VecDeque},
    fs::{File, OpenOptions},
    path::PathBuf,
};

use anyhow::Context;
use tokio::sync::mpsc;

use crate::{DBConnection, RequestPiecePayload, ResponsePiecePayload, Torrent};

mod file_manager;
mod req_preparer;

pub const BLOCK_QUEUE_SIZE_MAX: usize = 10;
/// how many pieces are in the queue at max
pub(self) const MAX_PIECES_IN_PARALLEL: usize = 2;

/// A message sent by a local peer to this Manager
#[derive(Debug, Clone)]
pub enum ReqMessage {
    NewConnection(([u8; 20], PeerConn)),
    PeerHas(Vec<bool>),
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
// - the ReqManager has access to all peers with a HashMap<PeerHash, mpsc::Sender<ResMessage>>, and maybe also the download speed
// - allows to implement:
//  - rarest-first-piece-selection
//  - peer not shutting down if the queue is empty, rather the Manager sends the shuttdown to all peers
//    (the peers have to send the shutdown back)
//  - remove the has-broadcaster from peers, Manager handles this
//  - choking: 4 active downloaders

#[derive(Debug, Clone)]
pub(super) enum ResMessage {
    NewBlockQueue(Vec<RequestPiecePayload>),
    Block(Option<ResponsePiecePayload>),
    WeHave(Vec<bool>),
    FinishedPiece(u32),
    FinishedFile,
}

pub struct ReqManager {
    /// the output file
    file: File,
    db_conn: DBConnection,
    rx: mpsc::Receiver<ReqMsgFromPeer>,
    /// I need this information too often to always query the DB
    /// so let's cache it
    have: Vec<bool>,
    /// if it's None, we are finished
    download_queue: Option<VecDeque<PieceState>>,
    pub torrent: Torrent,
    /// this is also cached, it'll never change
    info_hash: String,
    peers: HashMap<[u8; 20], PeerConn>,
}

#[derive(Debug, Clone)]
pub struct PeerConn {
    pub(super) sender: mpsc::Sender<ResMessage>,
    pub(super) has: Vec<bool>, // download_speed or something maybe
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

impl ReqManager {
    pub async fn init(
        rx: mpsc::Receiver<ReqMsgFromPeer>,
        file_path: Option<PathBuf>,
        torrent_path: PathBuf,
    ) -> anyhow::Result<Self> {
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
            .context("opening file")?;

        let download_state = if file_info.is_finished() {
            None
        } else {
            Some(VecDeque::with_capacity(MAX_PIECES_IN_PARALLEL))
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

    pub async fn run(&mut self) -> anyhow::Result<()> {
        while let Some(peer_msg) = self.rx.recv().await {
            let Some(peer) = self.peers.get(&peer_msg.peer_id) else {
                todo!("ignore it.");
            };
            match peer_msg.msg {
                ReqMessage::NewConnection((id, peer_conn)) => {
                    self.peers.insert(id, peer_conn);
                }
                ReqMessage::GotBlock(block) => {
                    if let Some(piece_index) = dbg!(self.write_block(block).await)? {
                        self.peers
                            .get(&peer_msg.peer_id)
                            .expect("we checked that before")
                            .sender
                            .send(ResMessage::FinishedPiece(piece_index))
                            .await
                            .context("sending have message to local peers")?;
                    };
                }
                ReqMessage::NeedBlock(block) => {
                    if self.have[block.index as usize] {
                        let block = self.get_block(block);
                        peer.sender
                            .send(ResMessage::Block(block))
                            .await
                            .context("sending block to peer")?;
                    } else {
                        todo!(
                            "send a message to the peer that we don't have the block? (not sure if there's a message type for this)"
                        );
                    }
                }
                ReqMessage::NeedBlockQueue => {
                    let blocks = self.prepare_next_blocks(BLOCK_QUEUE_SIZE_MAX, peer.has.clone());
                    self.peers
                        .get(&peer_msg.peer_id)
                        .expect("we checked that before")
                        .sender
                        .send(ResMessage::NewBlockQueue(blocks))
                        .await
                        .context("sending block queue")?;
                }
                ReqMessage::WhatDoWeHave => {
                    peer.sender
                        .send(ResMessage::WeHave(self.have.clone()))
                        .await
                        .context("send have")?;
                }
                ReqMessage::PeerHas(bitfield) => {
                    let peers_mut = self
                        .peers
                        .get_mut(&peer_msg.peer_id)
                        .expect("we checked that before");
                    peers_mut.has = bitfield;
                }
            }
        }

        Ok(())
    }
}
