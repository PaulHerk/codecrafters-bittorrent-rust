use std::{
    error::Error,
    net::{Ipv4Addr, SocketAddrV4},
    path::PathBuf,
};

use anyhow::Context;
use tokio::sync::mpsc;

use crate::{
    PEER_ID, PEER_PORT,
    database::DBConnection,
    magnet_links::{MagnetLink, MagnetLinkError},
    peer::Peer,
    peer_manager::{PeerManager, ReqMsgFromPeer},
    torrent::Torrent,
    tracker::TrackerRequest,
};

pub struct Client {
    db_conn: DBConnection,
}

impl Client {
    pub async fn new() -> Result<Self, Box<dyn Error>> {
        let db_conn = DBConnection::new().await?;
        Ok(Self { db_conn })
    }

    pub async fn download_torrent(
        &self,
        torrent_path: PathBuf,
        output_path: Option<PathBuf>,
    ) -> Result<(), Box<dyn Error>> {
        let torrent = Torrent::read_from_file(&torrent_path)?;
        self.start_download(torrent, output_path).await?;
        Ok(())
    }

    pub async fn download_magnet(
        &self,
        magnet_link_str: String,
        output_path: Option<PathBuf>,
    ) -> Result<(), Box<dyn Error>> {
        let magnet_link = MagnetLink::from_url(&magnet_link_str)?;
        let torrent = self.download_metadata(magnet_link).await?;
        self.start_download(torrent, output_path).await?;
        Ok(())
    }

    async fn download_metadata(&self, magnet_link: MagnetLink) -> Result<Torrent, MagnetLinkError> {
        // This will be implemented in the MetadataDownloader (formerly BeforePeerManager)
        // For now, it's a placeholder.
        // It will connect to peers, request metadata, and return a Torrent object.
        todo!("Implement metadata download from peers for magnet links");
    }

    async fn start_download(
        &self,
        torrent: Torrent,
        output_path: Option<PathBuf>,
    ) -> Result<(), Box<dyn Error>> {
        let (peer_manager_tx, peer_manager_rx) = mpsc::channel(64);

        let peer_manager = PeerManager::new(
            peer_manager_rx,
            output_path,
            torrent.clone(),
            self.db_conn.clone(),
        )
        .await?;

        let info_hash = torrent.info.info_hash();
        let tracker =
            TrackerRequest::new(&info_hash, PEER_ID, PEER_PORT, torrent.info.get_length());
        let response = tracker.get_response(torrent.announce).await?;

        tokio::spawn(async move {
            let _ = peer_manager.run().await;
        });

        for &addr in response.peers.0.iter() {
            let peer_manager_tx = peer_manager_tx.clone();
            tokio::spawn(async move {
                let peer = Peer::connect_from_addr(addr, info_hash, *PEER_ID, peer_manager_tx)
                    .await
                    .context("initializing peer")
                    .unwrap();
                peer.run().await.unwrap();
            });
        }

        // peer listener
        let addr = SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, PEER_PORT);
        let listener = tokio::net::TcpListener::bind(addr).await?;
        loop {
            let connection = listener.accept().await;
            let Ok((stream, _addr)) = connection else {
                continue;
            };
            let peer =
                Peer::connect_from_stream(stream, info_hash, *PEER_ID, peer_manager_tx.clone())
                    .await
                    .context("initializing incoming peer connection")
                    .unwrap();
            peer.run().await.unwrap();
        }
    }
}
