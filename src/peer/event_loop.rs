use futures_util::StreamExt;
use std::mem;

use crate::{
    extensions::BasicExtensionPayload,
    messages::{
        PeerMessage,
        payloads::{HavePayload, NoPayload},
    },
    peer::{Msg, Peer, error::PeerError},
    peer_manager::{ReqMessage, ResMessage},
};

impl Peer {
    pub async fn run(mut self) -> Result<(), PeerError> {
        // TODO: do choking
        *self.state.0.am_choking.lock().unwrap() = false;

        let mut receiver_stream = mem::take(&mut self.receiver_stream)
            .expect("The receiver stream is initialized after creation of the peer.");

        self.send_extended_handshake().await?;
        self.set_interested(true).await?;
        loop {
            if let Some(message) = receiver_stream.next().await {
                if let Msg::Data(PeerMessage::Piece(_)) = message {
                } else if let Msg::Data(PeerMessage::Extended(ref p)) = message
                    && p.extension_id == 2
                {
                } else {
                    // dbg!(&message);
                } /*else if let Msg::Manager(ResMessage::NewBlockQueue(_)) = message {
                dbg!(&message);
                }*/
                match message {
                    Msg::Manager(peer_msg) => match peer_msg {
                        ResMessage::FinishedFile => {
                            // TODO: set flags
                            if self.sever_conn().await {
                                break Ok(());
                            }
                        }
                        ResMessage::FinishedPiece(piece_index) => {
                            // later TODO: implement have suppression
                            let have_payload = HavePayload { piece_index };
                            self.send_peer(PeerMessage::Have(have_payload)).await?;
                        }
                        ResMessage::NewBlockQueue(request_piece_payloads) => {
                            dbg!(&request_piece_payloads);
                            let req_piece_payload_msgs: Vec<PeerMessage> = request_piece_payloads
                                .into_iter()
                                .map(PeerMessage::Request)
                                .collect();
                            self.queue
                                .to_send
                                .extend_from_slice(&req_piece_payload_msgs);

                            // TODO: keep interested state up-to-date
                            // if self.queue.to_send.is_empty() {
                            //     self.set_interested(false).await?;
                            // } else {
                            //     self.set_interested(true).await?;
                            // };
                        }
                        ResMessage::Block(response_piece_payload) => {
                            if let Some(payload) = response_piece_payload {
                                self.send_peer(PeerMessage::Piece(payload)).await?;
                            }
                            // TODO: else?
                        }
                        ResMessage::WeHave(bitfield) => {
                            // later TODO: implement lazy bitfield?
                            if !bitfield.is_empty() {
                                self.send_peer(PeerMessage::Bitfield(bitfield)).await?;
                            }
                        }
                        ResMessage::ExtensionData((ext_type, data)) => {
                            let msg = {
                                let extensions = self.state.0.extensions.lock().unwrap();
                                if let Some(extensions) = extensions.as_ref()
                                    && let Some((extension_id, _)) =
                                        extensions.iter().find(|d| d.1.get_ext_type() == ext_type)
                                {
                                    Some(PeerMessage::Extended(BasicExtensionPayload {
                                        extension_id: *extension_id,
                                        data,
                                    }))
                                } else {
                                    None
                                }
                            };
                            if let Some(msg) = dbg!(msg) {
                                self.queue.to_send.push(msg);
                                self.set_interested(true).await?;
                            }
                        }
                        ResMessage::StartDownload => {
                            self.send_peer_manager(ReqMessage::NeedBlockQueue).await?;
                        }
                    },
                    Msg::Data(message) => match message {
                        PeerMessage::Choke(_no_payload) => {
                            *self.state.0.peer_choking.lock().unwrap() = true;
                        }
                        PeerMessage::Unchoke(_no_payload) => {
                            eprintln!("PEER UNCHOKES");
                            *self.state.0.peer_choking.lock().unwrap() = false
                        }
                        PeerMessage::Interested(_no_payload) => {
                            *self.state.0.peer_interested.lock().unwrap() = true
                        }
                        PeerMessage::NotInterested(_no_payload) => {
                            *self.state.0.peer_interested.lock().unwrap() = false
                        }
                        PeerMessage::Have(have_payload) => {
                            self.state.0.has.lock().unwrap()[have_payload.piece_index as usize] =
                                true;
                        }
                        PeerMessage::Bitfield(bitfield_payload) => {
                            *self.state.0.has.lock().unwrap() = bitfield_payload.pieces_available;
                        }
                        PeerMessage::Request(request_piece_payload) => {
                            self.send_peer_manager(ReqMessage::NeedBlock(request_piece_payload))
                                .await?;
                        }
                        PeerMessage::Piece(response_piece_payload) => {
                            eprintln!(
                                "got {}th block in piece {}",
                                response_piece_payload.begin, response_piece_payload.index
                            );
                            self.queue.have_sent -= 1;
                            self.send_peer_manager(ReqMessage::GotBlock(response_piece_payload))
                                .await?;
                        }
                        PeerMessage::Cancel(_request_piece_payload) => todo!(),
                        PeerMessage::KeepAlive(_no_payload) => {
                            eprintln!("he sent a keep alive")
                        }
                        PeerMessage::Extended(extension_payload) => {
                            self.on_extension_data(extension_payload).await?;
                        }
                    },
                    Msg::Timeout => {
                        self.send_peer(PeerMessage::KeepAlive(NoPayload)).await?;
                    }
                }

                // request next blocks
                if self.queue.have_sent == 0
                    && *self.state.0.am_interested.lock().unwrap()
                    && !*self.state.0.peer_choking.lock().unwrap()
                {
                    let queue_iter: Vec<_> = mem::take(&mut self.queue.to_send);
                    self.queue.have_sent = queue_iter.len();
                    for req in queue_iter.into_iter() {
                        self.send_peer(req).await?;
                    }
                    eprintln!("need queue");
                    self.send_peer_manager(ReqMessage::NeedBlockQueue).await?;
                }
            } else {
                eprintln!("peer disconnected");
                break Err(PeerError::PeerDisconnected);
            }
        }?;
        self.receiver_stream = Some(receiver_stream);
        Ok(())
    }
}
