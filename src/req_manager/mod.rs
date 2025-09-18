use std::{
    collections::VecDeque,
    fs::{File, OpenOptions},
    path::PathBuf,
};

use anyhow::Context;
use tokio::sync::{mpsc, oneshot};

use crate::{DBConnection, HavePayload, RequestPiecePayload, ResponsePiecePayload, Torrent};

mod file_manager;
mod req_preparer;

pub const BLOCK_QUEUE_SIZE_MAX: usize = 10;
const BLOCK_QUEUE_SIZE_MIN: usize = 3;
/// how many pieces are in the queue at max
pub(self) const MAX_PIECES_IN_PARALLEL: usize = 2;

/// This enum will be sent to the ReqManager from a peer
#[derive(Debug)]
pub enum ReqMessage {
    NeedBlocksToReq {
        peer_has: Vec<bool>,
        tx: oneshot::Sender<Vec<RequestPiecePayload>>,
    },
    GotBlock {
        block: ResponsePiecePayload,
    },
    NeedBlock {
        block: RequestPiecePayload,
        tx: oneshot::Sender<Option<ResponsePiecePayload>>,
    },
    WhatDoWeHave {
        tx: oneshot::Sender<Vec<bool>>,
    },
}

// Next-up:
// - split it into ReqMessage and ResMessage (TODO: better naming)
// - the ReqManager has access to all peers with a HashMap<PeerHash, mpsc::Sender<ResMessage>>
// - allows to implement:
//  - rarest-first-piece-selection
//  - peer not shutting down if the queue is empty, rather the Manager sends the shuttdown to all peers
//    (the peers have to send the shutdown back)
//  - remove the has-broadcaster from peers, Manager handles this

pub struct ReqManager {
    /// the output file
    file: File,
    db_conn: DBConnection,
    rx: mpsc::Receiver<ReqMessage>,
    broadcaster: tokio::sync::broadcast::Sender<PeerMsg>,
    /// I need this information too often to always query the DB
    /// so let's cache it
    have: Vec<bool>,
    /// if it's None, we are finished
    download_queue: Option<VecDeque<PieceState>>,
    pub torrent: Torrent,
    /// this is also cached, it'll never change
    info_hash: String,
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

#[derive(Debug, Clone, Copy)]
pub enum PeerMsg {
    Shutdown,
    Have(HavePayload),
}

impl ReqManager {
    pub async fn init(
        rx: mpsc::Receiver<ReqMessage>,
        broadcaster: tokio::sync::broadcast::Sender<PeerMsg>,
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

        // // if the file didn't exist before, we want to fill it with all zeroes
        // if file_info.bitfield.iter().all(|b| *b == false) {
        //     file.set_len(torrent.get_length() as u64).await?;
        // }

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
            broadcaster,
            have: file_info.bitfield.to_vec(),
            download_queue: download_state,
            torrent,
            info_hash,
        })
    }

    pub async fn run(&mut self) -> anyhow::Result<()> {
        while let Some(msg) = self.rx.recv().await {
            match msg {
                ReqMessage::NeedBlocksToReq { tx, peer_has } => {
                    let blocks = self.prepare_next_blocks(BLOCK_QUEUE_SIZE_MAX, peer_has);
                    tx.send(dbg!(blocks)).unwrap();
                }
                ReqMessage::GotBlock { block } => {
                    if let Some(piece_index) = dbg!(self.write_block(block).await)? {
                        self.broadcaster
                            .send(PeerMsg::Have(HavePayload { piece_index }))
                            .context("sending have message to local peers")?;
                    };
                }
                ReqMessage::NeedBlock { block, tx } => {
                    if self.have[block.index as usize] {
                        let block = self.get_block(block);
                        tx.send(block).unwrap();
                    } else {
                        todo!(
                            "send a message to the peer that we don't have the block? (not sure if there's a message type for this)"
                        );
                    }
                }
                ReqMessage::WhatDoWeHave { tx } => {
                    tx.send(self.have.clone()).unwrap();
                }
            }
        }

        Ok(())
    }
}
