use std::collections::VecDeque;

use surrealdb::{Surreal, engine::local::Db};
use tokio::{
    fs::File,
    sync::{mpsc, oneshot},
};

use crate::{HavePayload, RequestPiecePayload, ResponsePiecePayload, Torrent};

mod file_manager;
mod req_preparer;

const BLOCK_QUEUE_SIZE: usize = 10;

/// This enum will be sent to the ReqManager from a peer
pub enum ReqMessage {
    NeedBlocksToReq {
        tx: oneshot::Sender<Vec<RequestPiecePayload>>,
    },
    GotBlock {
        block: ResponsePiecePayload,
    },
    NeedBlock {
        block: RequestPiecePayload,
        tx: oneshot::Sender<ResponsePiecePayload>,
    },
    WhatDoWeHave {
        tx: oneshot::Sender<Vec<bool>>,
    },
}

pub struct ReqManager {
    file: File,
    db_conn: Surreal<Db>,
    rx: mpsc::Receiver<ReqMessage>,
    broadcaster: tokio::sync::broadcast::Sender<PeerMsg>,
    /// I need this information too often to always query the DB
    /// so let's cache it
    have: Vec<bool>,
    /// if it's None, we are finished
    download_state: Option<VecDeque<PieceState>>,
    torrent: Torrent,
}

struct PieceState {
    bitfield: Vec<bool>,
    piece_i: u32,
    buf: Vec<u8>,
}

#[derive(Debug, Clone, Copy)]
pub enum PeerMsg {
    Shutdown,
    Have(HavePayload),
}

impl ReqManager {
    pub fn new(
        rx: mpsc::Receiver<ReqMessage>,
        broadcaster: tokio::sync::broadcast::Sender<PeerMsg>,
    ) -> Self {
        todo!()
    }

    pub async fn run(&mut self) {
        while let Some(msg) = self.rx.recv().await {
            match msg {
                ReqMessage::NeedBlocksToReq { tx } => {
                    let blocks = self.prepare_next_blocks(BLOCK_QUEUE_SIZE);
                    tx.send(blocks).unwrap();
                }
                ReqMessage::GotBlock { block } => {
                    self.write_block(block).await;
                }
                ReqMessage::NeedBlock { block, tx } => {
                    let have = self.get_have();
                    if have[block.index as usize] {
                        let block = self.get_block(block);
                        tx.send(block).unwrap();
                    } else {
                        todo!(
                            "send a message to the peer that we don't have the block (not sure if there's a message type for this)"
                        );
                    }
                }
                ReqMessage::WhatDoWeHave { tx } => {
                    let have = self.get_have();
                    tx.send(have).unwrap();
                }
            }
        }
    }
}
