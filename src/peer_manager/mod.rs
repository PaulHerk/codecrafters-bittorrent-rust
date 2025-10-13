use std::{collections::HashMap, path::PathBuf};

use tokio::sync::mpsc;

use crate::{
    Torrent,
    database::DBConnection,
    extensions::{
        ExtensionMessage, ExtensionType,
        magnet_links::{MagnetLink, metadata_piece_manager::MetadataPieceManager},
    },
    messages::payloads::{BitfieldPayload, RequestPiecePayload, ResponsePiecePayload},
    peer::conn::PeerState,
    peer_manager::{error::PeerManagerError, piece_manager::PieceManager},
    torrent::Metainfo,
};

pub mod error;
mod piece_manager;

pub const BLOCK_QUEUE_SIZE_MAX: usize = 20;
/// how many pieces are in the queue at max
pub(crate) const MAX_PIECES_IN_PARALLEL: usize = 5;

#[derive(Debug)]
pub struct PeerManager {
    torrent_state: TorrentState,
    rx: mpsc::Receiver<ReqMsgFromPeer>,
    announce_url: url::Url,
    peers: HashMap<[u8; 20], PeerConn>,
}

#[derive(Debug)]
enum TorrentState {
    // We are waiting for metadata.
    WaitingForMetadata {
        file_path: Option<PathBuf>,
        metadata_piece_manager: MetadataPieceManager, // A helper to track downloaded metadata pieces
    },
    // We have the metadata and can download the actual files.
    Downloading {
        metainfo: Metainfo,
        piece_manager: PieceManager,
    },
    // Optional: A seeding state
    Seeding {
        metainfo: Metainfo,
        // ... state relevant to seeding
    },
}

impl TorrentState {
    async fn from_info(
        db_conn: DBConnection,
        file_path: Option<PathBuf>,
        metainfo: Metainfo,
        announce: url::Url,
    ) -> Result<Self, PeerManagerError> {
        let torrent = Torrent {
            announce,
            info: metainfo,
        };
        Ok(TorrentState::Downloading {
            piece_manager: PieceManager::new(db_conn, file_path, &torrent).await?,
            metainfo: torrent.info,
        })
    }
}

/// A message sent by a local peer to this Manager
#[derive(Debug, Clone)]
pub enum ReqMessage {
    NewConnection(PeerConn),
    NeedBlockQueue,
    GotBlock(ResponsePiecePayload),
    NeedBlock(RequestPiecePayload),
    WhatDoWeHave,
    Extension(ExtensionMessage),
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
    /// indication to the peer to start the download loop
    StartDownload,
    NewBlockQueue(Vec<RequestPiecePayload>),
    Block(Option<ResponsePiecePayload>),
    WeHave(BitfieldPayload),
    FinishedPiece(u32),
    FinishedFile,
    /// Data that is passed to BasicExtensionPayload.
    /// The peer has to 'add' the extended_msg_id itself since it is peer-dependent
    ExtensionData((ExtensionType, Vec<u8>)),
}

#[derive(Debug, Clone)]
pub struct PeerConn {
    pub(super) sender: mpsc::Sender<ResMessage>,
    pub(super) identifier: PeerState,
}

impl PeerConn {
    async fn send(&self, msg: ResMessage, peer_id: [u8; 20]) -> Result<(), PeerManagerError> {
        self.sender
            .send(msg)
            .await
            .map_err(|error| PeerManagerError::SendError {
                peer_id,
                error,
                msg: "sending a message".to_string(),
            })
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PieceState {
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
    pub async fn init_from_magnet(
        rx: mpsc::Receiver<ReqMsgFromPeer>,
        file_path: Option<PathBuf>,
        magnet_link: MagnetLink,
    ) -> Result<Self, PeerManagerError> {
        let db_conn = DBConnection::new(magnet_link.info_hash).await?;
        if let Some(file_entry) = db_conn.get_entry().await? {
            Ok(Self {
                torrent_state: TorrentState::from_info(
                    db_conn,
                    Some(file_entry.file.to_path_buf()),
                    file_entry.torrent_info,
                    file_entry.announce.clone(),
                )
                .await?,
                rx,
                announce_url: file_entry.announce,
                peers: HashMap::new(),
            })
        } else {
            let torrent_state = TorrentState::WaitingForMetadata {
                file_path,
                metadata_piece_manager: MetadataPieceManager::new(magnet_link.info_hash),
            };
            Ok(Self {
                torrent_state,
                rx,
                announce_url: magnet_link.get_announce_url()?,
                peers: HashMap::new(),
            })
        }
    }

    pub async fn init_from_torrent(
        rx: mpsc::Receiver<ReqMsgFromPeer>,
        file_path: Option<PathBuf>,
        torrent: Torrent,
    ) -> Result<Self, PeerManagerError> {
        let info_hash = torrent.info.info_hash();
        let db_conn = DBConnection::new(info_hash).await?;
        let torrent_state =
            TorrentState::from_info(db_conn, file_path, torrent.info, torrent.announce.clone())
                .await?;

        Ok(Self {
            torrent_state,
            rx,
            announce_url: torrent.announce,
            peers: HashMap::new(),
        })
    }

    pub async fn run(mut self) -> Result<(), PeerManagerError> {
        while let Some(peer_msg) = self.rx.recv().await {
            match peer_msg.msg {
                ReqMessage::NewConnection(peer_conn) => {
                    self.peers.insert(peer_msg.peer_id, peer_conn);

                    if let TorrentState::Downloading {
                        metainfo: _,
                        piece_manager: _,
                    } = self.torrent_state
                    {
                        self.send_peer(peer_msg.peer_id, ResMessage::StartDownload)
                            .await?;
                    }
                }
                ReqMessage::GotBlock(block) => {
                    if let TorrentState::Downloading {
                        metainfo,
                        piece_manager,
                    } = &mut self.torrent_state
                        && let Some(piece_index) =
                            piece_manager.write_block(block, metainfo).await?
                    {
                        let msg = ResMessage::FinishedPiece(piece_index);
                        eprintln!("Finished piece number {piece_index}.");
                        if piece_manager.is_finished() {
                            self.torrent_state = TorrentState::Seeding {
                                metainfo: metainfo.clone(),
                            };
                            self.broadcast_peers(ResMessage::FinishedFile).await?;
                        }
                        self.broadcast_peers(msg).await?;
                    }
                }
                ReqMessage::NeedBlock(block) => {
                    if let TorrentState::Downloading {
                        metainfo,
                        piece_manager,
                    } = &self.torrent_state
                    {
                        if piece_manager.have[block.index as usize] {
                            let block = piece_manager.get_block(block, metainfo);
                            let msg = ResMessage::Block(block);
                            self.send_peer(peer_msg.peer_id, msg).await?;
                        } else {
                            todo!(
                                "send a message to the peer that we don't have the block? (not sure if there's a message type for this)"
                            );
                        }
                    }
                }
                ReqMessage::NeedBlockQueue => {
                    let peer_has = self.get_peer_has(&peer_msg.peer_id)?;
                    if let TorrentState::Downloading {
                        metainfo,
                        piece_manager,
                    } = &mut self.torrent_state
                    {
                        let blocks = piece_manager.prepare_next_blocks(
                            BLOCK_QUEUE_SIZE_MAX,
                            &peer_has,
                            metainfo,
                        );
                        let msg = ResMessage::NewBlockQueue(blocks);
                        self.send_peer(peer_msg.peer_id, msg).await?;
                    }
                }
                ReqMessage::WhatDoWeHave => {
                    if let TorrentState::Downloading {
                        metainfo: _,
                        piece_manager,
                    } = &self.torrent_state
                    {
                        let msg = ResMessage::WeHave(BitfieldPayload {
                            pieces_available: piece_manager.have.clone(),
                        });
                        self.send_peer(peer_msg.peer_id, msg).await?;
                    }
                }
                ReqMessage::Extension(extension_message) => {
                    if let TorrentState::WaitingForMetadata {
                        file_path,
                        metadata_piece_manager,
                    } = &mut self.torrent_state
                    {
                        match extension_message {
                            ExtensionMessage::NeedMetadataPiece => {
                                let msg = get_metadata_msg(metadata_piece_manager)?;
                                if let Some(msg) = msg {
                                    self.send_peer(peer_msg.peer_id, msg).await?;
                                }
                            }
                            ExtensionMessage::ReceivedMetadataPiece { piece_index, data } => {
                                metadata_piece_manager.add_block(piece_index, data.to_vec())?;
                                if metadata_piece_manager.check_finished() {
                                    let metainfo = metadata_piece_manager.get_metadata().expect("This shouldn't fail since we checked that the hashes match.");
                                    dbg!("Finished downloading metainfo", &metainfo);
                                    let torrent = Torrent {
                                        announce: self.announce_url.clone(),
                                        info: metainfo,
                                    };
                                    let piece_manager = PieceManager::new(
                                        DBConnection::new(metadata_piece_manager.info_hash).await?,
                                        file_path.clone(),
                                        &torrent,
                                    )
                                    .await?;
                                    self.torrent_state = TorrentState::Downloading {
                                        metainfo: torrent.info,
                                        piece_manager,
                                    };
                                    self.broadcast_peers(ResMessage::StartDownload).await?;
                                }
                            }
                            ExtensionMessage::GotMetadataLength(length) => {
                                metadata_piece_manager.set_len(length);
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn send_peer(
        &mut self,
        peer_id: [u8; 20],
        msg: ResMessage,
    ) -> Result<(), PeerManagerError> {
        let peer = self
            .peers
            .get_mut(&peer_id)
            .ok_or(PeerManagerError::PeerNotFound)?;
        peer.send(msg, peer_id).await
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

    async fn broadcast_peers(&mut self, msg: ResMessage) -> Result<(), PeerManagerError> {
        for (&peer_id, conn) in self.peers.iter() {
            conn.send(msg.clone(), peer_id).await?;
        }

        Ok(())
    }
}

/// helper function that get's the new blocks to be added and creates a message of it
fn get_metadata_msg(
    metadata_piece_manager: &mut MetadataPieceManager,
) -> Result<Option<ResMessage>, PeerManagerError> {
    let new_data = metadata_piece_manager
        .get_block_req_data()
        .map_err(|e| PeerManagerError::Other(Box::new(e)))?;

    if let Some(data) = new_data {
        Ok(Some(ResMessage::ExtensionData((
            ExtensionType::Metadata,
            data,
        ))))
    } else {
        Ok(None)
    }
}
