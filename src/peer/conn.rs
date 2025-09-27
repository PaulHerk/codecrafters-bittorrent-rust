use std::net::SocketAddrV4;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::Context;
use futures_core::Stream;
use futures_util::StreamExt;
use futures_util::stream::SplitSink;
use futures_util::stream::SplitStream;
use futures_util::stream::unfold;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::Sender;
use tokio_util::codec::Framed;
use tokio_util::time::FutureExt;

use crate::messages::payloads::RequestPiecePayload;
use crate::messages::{MessageFramer, PeerMessage};
use crate::peer::Msg;
use crate::peer::handshake::Handshake;
use crate::req_manager::PeerConn;
use crate::req_manager::ReqMessage;
use crate::req_manager::ReqMsgFromPeer;
use crate::req_manager::ResMessage;

/// this is just a wrapper type for the actual states that wraps it in an Arc
#[derive(Debug, Clone)]
pub(crate) struct PeerState(pub(crate) Arc<PeerStateInner>);

#[derive(Debug)]
pub(crate) struct PeerStateInner {
    pub peer_id: [u8; 20],
    // dk if I need this at all
    // pub state: Arc<Mutex<super::PeerState>>,
    pub am_choking: Mutex<bool>,
    pub am_interested: Mutex<bool>,
    pub peer_choking: Mutex<bool>,
    pub peer_interested: Mutex<bool>,
    /// the bitfield of the other peer
    pub has: Mutex<Vec<bool>>,
}

impl PeerState {
    pub(crate) fn new(peer_id: [u8; 20]) -> Self {
        let peer_identifier_inner = PeerStateInner {
            peer_id,
            am_choking: Mutex::new(true),
            am_interested: Mutex::new(false),
            peer_choking: Mutex::new(true),
            peer_interested: Mutex::new(false),
            has: Mutex::new(Vec::new()),
        };
        Self(Arc::new(peer_identifier_inner))
    }
}

pub(super) type BoxedMsgStream = Pin<Box<dyn Stream<Item = Msg> + Send>>;
type PeerWriter = SplitSink<Framed<TcpStream, MessageFramer>, PeerMessage>;
type PeerReader = SplitStream<Framed<TcpStream, MessageFramer>>;

pub struct Peer {
    pub(super) state: PeerState,
    pub(super) req_queue: Vec<RequestPiecePayload>,
    pub(super) req_manager_tx: mpsc::Sender<ReqMsgFromPeer>,
    pub(super) peer_writer: PeerWriter,
}

impl Peer {
    pub async fn connect_from_addr(
        addr: SocketAddrV4,
        info_hash: [u8; 20],
        peer_id: [u8; 20],
        req_manager_tx: Sender<ReqMsgFromPeer>,
    ) -> anyhow::Result<(Self, BoxedMsgStream)> {
        // set up tcp connection & shake hands
        let tcp = tokio::net::TcpStream::connect(addr)
            .await
            .context("establishing tcp connection")?;

        Peer::connect_from_stream(tcp, info_hash, peer_id, req_manager_tx).await
    }

    pub async fn connect_from_stream(
        mut tcp: TcpStream,
        info_hash: [u8; 20],
        peer_id: [u8; 20],
        req_manager_tx: Sender<ReqMsgFromPeer>,
    ) -> anyhow::Result<(Self, BoxedMsgStream)> {
        let handshake_recv = Handshake::new(info_hash, peer_id)
            .shake_hands(&mut tcp)
            .await
            .context("shaking hands with peer")?;
        println!("peer {} connected", tcp.peer_addr().unwrap());

        let peer_identifier = PeerState::new(handshake_recv.peer_id);

        // after the handshake as succeeded we can create the message framer that de- & encodes the messages
        // from the tcp stream
        let framed = Framed::new(tcp, MessageFramer);

        // set up req_manager connection
        let (sender, req_manager_rx) = mpsc::channel(16);
        let peer_conn = PeerConn {
            sender,
            identifier: peer_identifier.clone(),
        };
        req_manager_tx
            .send(ReqMsgFromPeer {
                peer_id: peer_identifier.0.peer_id,
                msg: ReqMessage::NewConnection(peer_conn),
            })
            .await
            .context("sending the new connection to the ReqManager")?;

        let (peer_writer, peer_reader) = framed.split();
        let receiver_stream = get_stream(peer_reader, req_manager_rx).await;

        Ok((
            Self {
                state: peer_identifier,
                req_queue: Vec::new(),
                req_manager_tx,
                peer_writer,
            },
            receiver_stream,
        ))
    }
}

async fn get_stream(framed_rx: PeerReader, req_manager_rx: Receiver<ResMessage>) -> BoxedMsgStream {
    let peer_msg_stream = unfold(framed_rx, |mut framed| async move {
        match framed.next().timeout(Duration::from_secs(120)).await {
            Ok(Some(Ok(message))) => Some((Msg::Data(message), framed)),
            Err(_) => Some((Msg::Timeout, framed)),
            Ok(None) => {
                // nothing really happens here
                None
            }
            Ok(Some(Err(e))) => {
                panic!("Error occured on PeerReader: {e:?}")
            }
        }
    });

    // this is the stream sent by other connections to peers to send have messages
    let manager_stream = unfold(req_manager_rx, |mut rx| async move {
        let msg = rx.recv().await?;
        Some((Msg::ManagerMsg(msg), rx))
    });

    let stream = futures_util::stream::select(peer_msg_stream, manager_stream);
    Box::pin(stream)
}
