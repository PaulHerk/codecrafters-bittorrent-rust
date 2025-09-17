use std::{collections::VecDeque, path::PathBuf};

use anyhow::Context;
use surrealdb::{Surreal, engine::local::Db};
use tokio::{
    fs::File,
    sync::{mpsc, oneshot},
};

use crate::{DBConnection, HavePayload, RequestPiecePayload, ResponsePiecePayload, Torrent};

mod file_manager;
mod req_preparer;

pub const BLOCK_QUEUE_SIZE: usize = 10;

/// This enum will be sent to the ReqManager from a peer
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
        tx: oneshot::Sender<ResponsePiecePayload>,
    },
    WhatDoWeHave {
        tx: oneshot::Sender<Vec<bool>>,
    },
}

pub struct ReqManager {
    /// the output file
    file: File,
    db_conn: Surreal<Db>,
    rx: mpsc::Receiver<ReqMessage>,
    broadcaster: tokio::sync::broadcast::Sender<PeerMsg>,
    /// I need this information too often to always query the DB
    /// so let's cache it
    have: Vec<bool>,
    /// if it's None, we are finished
    download_state: Option<VecDeque<PieceState>>,
    pub torrent: Torrent,
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

        let file = tokio::fs::File::open(&file_info.file)
            .await
            .context("opening file")?;

        let download_state = if file_info.is_finished() {
            None
        } else {
            Some(VecDeque::with_capacity(2))
        };

        Ok(Self {
            file,
            db_conn: db_conn.0,
            rx,
            broadcaster,
            have: file_info.bitfield.to_vec(),
            download_state,
            torrent,
        })
    }

    pub async fn run(&mut self) -> anyhow::Result<()> {
        while let Some(msg) = self.rx.recv().await {
            match msg {
                ReqMessage::NeedBlocksToReq { tx, peer_has } => {
                    let blocks = self.prepare_next_blocks(BLOCK_QUEUE_SIZE, peer_has);
                    tx.send(blocks).unwrap();
                }
                ReqMessage::GotBlock { block } => {
                    if let Some(piece_index) = self.write_block(block).await {
                        self.broadcaster
                            .send(PeerMsg::Have(HavePayload { piece_index }))
                            .context("sending have message to local peers")?;
                    };
                }
                ReqMessage::NeedBlock { block, tx } => {
                    let have = self.get_have();
                    if have[block.index as usize] {
                        let block = self.get_block(block);
                        tx.send(block).unwrap();
                    } else {
                        todo!(
                            "send a message to the peer that we don't have the block? (not sure if there's a message type for this)"
                        );
                    }
                }
                ReqMessage::WhatDoWeHave { tx } => {
                    let have = self.get_have();
                    tx.send(have).unwrap();
                }
            }
        }

        Ok(())
    }
}
