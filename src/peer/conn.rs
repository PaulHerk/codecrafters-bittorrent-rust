use std::collections::HashMap;
use std::mem;
use std::net::SocketAddrV4;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

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

use crate::messages::extensions::ExtensionType;
use crate::messages::{MessageFramer, PeerMessage};
use crate::peer::Msg;
use crate::peer::Peer;
use crate::peer::error::PeerError;
use crate::peer::handshake::Handshake;
use crate::peer_manager::PeerConn;
use crate::peer_manager::ReqMessage;
use crate::peer_manager::ReqMsgFromPeer;
use crate::peer_manager::ResMessage;

/// this is just a wrapper type for the actual states that wraps it in an Arc
#[derive(Debug, Clone)]
pub(crate) struct PeerState(pub(crate) Arc<PeerStateInner>);

#[derive(Debug)]
pub(crate) struct PeerStateInner {
    /// the peer_id of the remote peer
    pub(crate) peer_id: [u8; 20],
    // dk if I need this at all
    // pub state: Arc<Mutex<super::PeerState>>,
    pub(crate) am_choking: Mutex<bool>,
    pub(crate) am_interested: Mutex<bool>,
    pub(crate) peer_choking: Mutex<bool>,
    pub(crate) peer_interested: Mutex<bool>,
    /// the bitfield of the other peer
    pub(crate) has: Mutex<Vec<bool>>,
    /// maps extended message ID to names of extensions
    pub(crate) extensions: Mutex<HashMap<u8, ExtensionType>>,
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
            extensions: Mutex::new(HashMap::new()),
        };
        Self(Arc::new(peer_identifier_inner))
    }

    async fn connect_to_peer_manager(
        &self,
        peer_manager_tx: &Sender<ReqMsgFromPeer>,
    ) -> Result<Receiver<ResMessage>, PeerError> {
        let (sender, peer_manager_rx) = mpsc::channel(16);
        let peer_conn = PeerConn {
            sender,
            identifier: self.clone(),
        };
        let peer_id = self.0.peer_id;
        let msg = ReqMsgFromPeer {
            peer_id,
            msg: ReqMessage::NewConnection(peer_conn),
        };
        send_peer_manager(peer_manager_tx, msg, peer_id).await?;

        Ok(peer_manager_rx)
    }
}

pub(super) type BoxedMsgStream = Pin<Box<dyn Stream<Item = Msg> + Send + Sync>>;
pub(super) type PeerWriter = SplitSink<Framed<TcpStream, MessageFramer>, PeerMessage>;
type PeerReader = SplitStream<Framed<TcpStream, MessageFramer>>;

impl Peer {
    pub async fn connect_from_addr(
        addr: SocketAddrV4,
        info_hash: [u8; 20],
        peer_id: [u8; 20],
        peer_manager_tx: Sender<ReqMsgFromPeer>,
    ) -> Result<Self, PeerError> {
        // set up tcp connection & shake hands
        let tcp = tokio::net::TcpStream::connect(addr)
            .await
            .map_err(|error| PeerError::FailedToConnect { error, addr })?;

        Peer::connect_from_stream(tcp, info_hash, peer_id, peer_manager_tx).await
    }

    pub async fn connect_from_stream(
        mut tcp: TcpStream,
        info_hash: [u8; 20],
        peer_id: [u8; 20],
        peer_manager_tx: Sender<ReqMsgFromPeer>,
    ) -> Result<Self, PeerError> {
        let handshake_recv = Handshake::new(info_hash, peer_id)
            .shake_hands(&mut tcp)
            .await?;
        println!("peer {} connected", tcp.peer_addr().unwrap());

        let peer_state = PeerState::new(handshake_recv.peer_id);

        // after the handshake as succeeded we can create the message framer that de- & encodes the messages
        // from the tcp stream
        let framed = Framed::new(tcp, MessageFramer);

        // set up peer_manager connection
        let peer_manager_rx = peer_state.connect_to_peer_manager(&peer_manager_tx).await?;
        let (peer_writer, peer_reader) = framed.split();
        let receiver_stream = Some(get_stream(peer_reader, peer_manager_rx).await);

        Ok(Self {
            state: peer_state,
            req_queue: Vec::new(),
            peer_manager_tx,
            peer_writer,
            receiver_stream,
        })
    }
}

async fn get_stream(
    framed_rx: PeerReader,
    peer_manager_rx: Receiver<ResMessage>,
) -> BoxedMsgStream {
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
    let manager_stream = unfold(peer_manager_rx, |mut rx| async move {
        let msg = rx.recv().await?;
        Some((Msg::Manager(msg), rx))
    });

    let stream = futures_util::stream::select(peer_msg_stream, manager_stream);
    Box::pin(stream)
}

pub(super) async fn send_peer_manager(
    peer_manager_tx: &Sender<ReqMsgFromPeer>,
    msg: ReqMsgFromPeer,
    peer_id: [u8; 20],
) -> Result<(), PeerError> {
    let msg_type = mem::discriminant(&msg.msg);
    peer_manager_tx
        .send(msg)
        .await
        .map_err(|error| PeerError::SendToPeerManager {
            error,
            peer_id,
            msg_type,
        })
}
