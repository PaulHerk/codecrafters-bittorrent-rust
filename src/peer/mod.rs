use futures_util::{SinkExt, StreamExt};
use sha1::{Digest, Sha1};
use std::net::SocketAddrV4;
use std::sync::{Arc, Mutex};
use tokio_util::codec::Framed;

use anyhow::Context;

use crate::messages::payloads::*;
use crate::states::PeerState;
use crate::{messages::*, *};

pub mod handshake;
pub mod states;

/// this struct is passed to all the peers
#[derive(Debug, Clone)]
pub struct PeerData {
    torrent: Arc<Mutex<Torrent>>,
    /// multiple pieces
    /// each piece is a Vec of blocks
    /// each block is a Vec of bytes
    pieces: Arc<Mutex<Vec<Vec<Vec<u8>>>>>,
    have: Arc<Mutex<Vec<PieceState>>>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum BlockState {
    None,
    InProgress,
    Complete,
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum PieceState {
    Incomplete(Vec<BlockState>),
    Complete,
}
impl PieceState {
    fn is_complete(&self) -> bool {
        match self {
            PieceState::Complete => true,
            PieceState::Incomplete(blocks) => blocks.iter().all(|b| *b == BlockState::Complete),
        }
    }
}

impl PeerData {
    pub fn new(torrent: Torrent, bytes: &[u8]) -> Self {
        let length = torrent.get_length();

        let mut pieces: Vec<Vec<Vec<u8>>> = Vec::with_capacity(torrent.info.pieces.0.len());
        let mut have = Vec::with_capacity(torrent.info.pieces.0.len());

        // what this loop does:
        // 1. put the bytes into the pieces Vec
        // 2. allocate the exact space needed if the bytes are yet to be recieved
        // 3. initialize the have Vec
        for piece_i in 0..torrent.info.pieces.0.len() as u32 {
            let piece_size = if piece_i == torrent.info.pieces.0.len() as u32 - 1
                && length % torrent.info.piece_length != 0
            {
                length % torrent.info.piece_length
            } else {
                torrent.info.piece_length
            };
            // the "+ BLOCK_MAX - 1" rounds up
            let nblocks = (piece_size + BLOCK_MAX - 1) / BLOCK_MAX;
            eprintln!("{nblocks} blocks of at most {BLOCK_MAX} to reach {piece_size}");

            pieces.push(Vec::with_capacity(piece_size as usize));
            have.push(PieceState::Incomplete(vec![
                BlockState::None;
                nblocks as usize
            ]));

            for block_i in 0..nblocks {
                let block_length = if block_i == nblocks - 1 && piece_size % BLOCK_MAX != 0 {
                    piece_size % BLOCK_MAX
                } else {
                    BLOCK_MAX
                };

                let range =
                    (block_i * BLOCK_MAX) as usize..(block_i * BLOCK_MAX + block_length) as usize;

                pieces[piece_i as usize].push(Vec::with_capacity(block_length as usize));
                if let Some(block) = bytes.get(range) {
                    pieces[piece_i as usize][block_i as usize].copy_from_slice(block);
                    let PieceState::Incomplete(blocks) = &mut have[piece_i as usize] else {
                        unreachable!()
                    };
                    blocks[block_i as usize] = BlockState::Complete;
                }
                if have[piece_i as usize].is_complete() {
                    have[piece_i as usize] = PieceState::Complete;
                }
            }
        }
        Self {
            torrent: Arc::new(Mutex::new(torrent)),
            pieces: Arc::new(Mutex::new(pieces)),
            have: Arc::new(Mutex::new(have)),
        }
    }
    /// returns the next piece and block begin that we need to request
    /// if we have all the blocks of a piece, we return None
    fn prepare_next_req_send(&self) -> Option<RequestPiecePayload> {
        // TODO: consider which blocks are already in progress
        let have = &mut self.have.lock().unwrap();
        let (piece_i, blocks_state) =
            have.iter_mut()
                .enumerate()
                .find_map(|(piece_i, piece_state)| match piece_state {
                    PieceState::Incomplete(blocks)
                        if blocks.iter().find(|b| *b == &BlockState::None).is_some() =>
                    {
                        Some((piece_i, blocks))
                    }
                    _ => None,
                })?;
        let (block_i, block_state) = blocks_state
            .iter_mut()
            .enumerate()
            .find(|(_i, b)| *b == &BlockState::None)
            .expect("there should be an empty if there's a piece that is incomplete");
        let block_i = block_i as u32;
        let block_begin = block_i * BLOCK_MAX;
        let block_len = self.pieces.lock().unwrap()[piece_i][block_i as usize].capacity() as u32;

        *block_state = BlockState::InProgress;

        let req = RequestPiecePayload::new(piece_i as u32, block_begin, block_len);

        Some(req)
    }
    fn add_block(&self, payload: ResponsePiecePayload) -> anyhow::Result<()> {
        // the "+ BLOCK_MAX - 1" rounds up
        let block_i = (payload.begin + BLOCK_MAX - 1) / BLOCK_MAX;

        let piece = &mut self.pieces.lock().unwrap()[payload.index as usize];
        piece[block_i as usize].append(&mut payload.block.to_vec());

        let have = &mut self.have.lock().unwrap()[payload.index as usize];
        let PieceState::Incomplete(blocks_state) = have else {
            unreachable!()
        };
        blocks_state[block_i as usize] = BlockState::Complete;

        // calculate the hash of the torrent if we have a piece
        let torrent = self.torrent.lock().unwrap();
        if have.is_complete() && have != &PieceState::Complete {
            // the second condition should always be false
            eprintln!("piece {} complete", payload.index);
            let mut sha1 = Sha1::new();
            sha1.update(piece.concat());
            let hash: [u8; 20] = sha1
                .finalize()
                .try_into()
                .expect("GenericArray<_, 20> == [u8; 20]");
            let torrent_hash = torrent.info.pieces.0[payload.index as usize];
            dbg!(payload.index);
            assert_eq!(hash, torrent_hash);

            println!("piece {} complete", payload.index);
            *have = PieceState::Complete;
        }
        Ok(())
    }

    pub fn get_data(&self) -> Option<Vec<u8>> {
        if !self.have.lock().unwrap().iter().all(|b| b.is_complete()) {
            return None;
        };

        let mut data = Vec::new();
        for piece in self.pieces.lock().unwrap().iter() {
            for block in piece.iter() {
                data.extend_from_slice(block.as_slice());
            }
        }
        Some(data)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Peer {
    pub peer_id: [u8; 20],
    pub addr: SocketAddrV4,
    pub state: PeerState,
    pub we_are_choked: bool,
    pub we_are_interested: bool,
    pub is_choking: bool,
    pub is_interested: bool,
}

impl Peer {
    pub fn new(peer_id: [u8; 20], addr: SocketAddrV4) -> Self {
        Self {
            peer_id,
            addr,
            state: PeerState::default(),
            we_are_choked: true,
            we_are_interested: false,
            is_choking: true,
            is_interested: false,
        }
    }
    /// info_hash: the info hash of the torrent
    /// peer_id: a unique identifier for your client
    pub async fn shake_hands_get_framed(
        &mut self,
        info_hash: [u8; 20],
    ) -> anyhow::Result<MsgFrameType> {
        let mut tcp = tokio::net::TcpStream::connect(self.addr)
            .await
            .context("connect to peer")?;
        self.state = PeerState::Connected;

        let handshake_to_send = Handshake::new(info_hash, self.peer_id);
        let handshake_recv = handshake_to_send
            .shake_hands(&mut tcp)
            .await
            .expect("peer should send handshake");

        assert_eq!(handshake_recv.info_hash, info_hash); // TODO: sever connection
        // assert_eq!(handshake_recv.peer_id, self.peer_id);

        self.state = PeerState::DataTransfer;

        let framed = Framed::new(tcp, MessageFramer);

        println!("peer {} connected", self.addr);
        Ok(framed)
    }

    pub async fn event_loop(
        &mut self,
        mut framed: MsgFrameType,
        peer_data: PeerData,
    ) -> anyhow::Result<()> {
        assert_eq!(self.state, PeerState::DataTransfer);
        // TODO: check if we are interested or not
        self.we_are_choked = false;

        // TODO: check if downloading or uploading?
        let bitfield: Message = framed
            .next()
            .await
            .expect("peer should send bitfield")
            .context("peer msg was invalid")?;
        assert_eq!(bitfield.get_msg_type(), MessageType::Bitfield);
        // NOTE: we assume that all the peers have all the data

        let interested_msg: Message = Message::new(MessageAll::Interested(NoPayload));
        framed
            .send(interested_msg)
            .await
            .context("write interested frame")?;

        self.we_are_interested = true;

        let req = peer_data
            .prepare_next_req_send()
            .expect("we should have pieces to request");
        framed
            .send(Message::new(MessageAll::Request(req)))
            .await
            .context("send request")?;

        loop {
            let Ok(Ok(message)) = framed.next().await.context("read message") else {
                continue;
            };
            match message.payload {
                MessageAll::Choke(_no_payload) => {
                    self.is_choking = true;
                    return Err(anyhow::anyhow!("peer is choked"));
                }
                MessageAll::Unchoke(_no_payload) => self.is_choking = false,
                MessageAll::Interested(_no_payload) => self.is_interested = true,
                MessageAll::NotInterested(_no_payload) => self.is_interested = false,
                MessageAll::Have(have_payload) => todo!(),
                MessageAll::Bitfield(bitfield_payload) => todo!(),
                MessageAll::Request(request_piece_payload) => todo!(),
                MessageAll::Piece(response_piece_payload) => {
                    peer_data.add_block(response_piece_payload)?;
                    if let Some(req) = peer_data.prepare_next_req_send() {
                        framed
                            .send(Message::new(MessageAll::Request(req)))
                            .await
                            .context("send request")?;
                    } else {
                        // let's pretend we are done
                        return Ok(());
                    }
                }
                MessageAll::Cancel(request_piece_payload) => todo!(),
                MessageAll::KeepAlive(no_payload) => todo!(),
            }
        }
    }
}
