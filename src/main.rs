use anyhow::Context;
use clap::{Parser, Subcommand};
use codecrafters_bittorrent::{Peer, PeerManager, Torrent, TrackerRequest};
use std::error::Error;
use std::net::{Ipv4Addr, SocketAddrV4};
use std::path::PathBuf;
use tokio::sync::mpsc;

const PEER_ID: &[u8; 20] = b"-AZ2060-222222222222";
const PEER_PORT: u16 = 6881;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: DecodeMetadataType,
}
#[derive(Debug, Subcommand)]
#[clap(rename_all = "snake_case")]
enum DecodeMetadataType {
    Decode {
        value: String,
    },
    Info {
        torrent: PathBuf,
    },
    Peers {
        torrent: PathBuf,
    },
    Handshake {
        torrent: PathBuf,
        addr: SocketAddrV4,
    },
    DownloadPiece {
        #[arg(short)]
        output: PathBuf,
        torrent: PathBuf,
        piece: u32,
    },
    Download {
        #[arg(short)]
        output: Option<PathBuf>,
        torrent: PathBuf,
    },
}

// Usage: your_program.sh decode "<encoded_value>"
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    // You can check for the existence of subcommands, and if found use their
    // matches just as you would the top level cmd
    match &cli.command {
        DecodeMetadataType::Decode { value } => {
            let decoded_value: serde_bencode::value::Value =
                serde_bencode::from_str(value).context("decode bencode")?;
            println!("{decoded_value:?}");
        }
        DecodeMetadataType::Info { torrent } => {
            let torrent = Torrent::read_from_file(torrent)?;
            // println!("Tracker URL: {}", torrent.announce);
            // println!("Length: {}", torrent.get_length());
            let info_hash = torrent.info_hash();
            println!("Info Hash: {}", hex::encode(info_hash));
            println!("Piece Length: {}", torrent.info.piece_length);
            // print everything except the piece hashes
            println!("{:#?}", torrent.info.files);
            println!("{:#?}", torrent.info.name);
            println!("{:#?}", torrent.info.other);
        }
        DecodeMetadataType::Peers { torrent } => {
            let torrent = Torrent::read_from_file(torrent)?;
            let info_hash = torrent.info_hash();
            let tracker_req =
                TrackerRequest::new(&info_hash, PEER_ID, PEER_PORT, torrent.get_length());
            let response = tracker_req.get_response(&torrent.announce).await?;
            for peer in response.peers.0 {
                println!("{peer:?}");
            }
        }
        DecodeMetadataType::Handshake { torrent, addr } => {
            let torrent = Torrent::read_from_file(torrent)?;
            let (tx, _rx) = mpsc::channel(1);
            let peer = Peer::connect_from_addr(*addr, torrent.info_hash(), *PEER_ID, tx).await?;
            println!("Peer with id {:?} connected", peer.get_id());
        }
        DecodeMetadataType::DownloadPiece {
            output: _,
            torrent: _,
            piece: _piece_i,
        } => {
            // let file = DBFile {
            //     file_path: PathBuf::from("./test.txt"),
            //     torrent_path: PathBuf::from("./sample.torrent"),
            // };
            // let (has_tx, _has_rx) = tokio::sync::broadcast::channel(32);

            // let file_loader = FileLoader::from_db_file(file).await?;
            // if file_loader.is_finished() {
            //     println!("finished");
            //     return Ok(());
            // }
            // let response =
            //     get_response(&file_loader.torrent, file_loader.torrent.get_length()).await?;
            // let info_hash = file_loader.torrent.info_hash()?;

            // for peer in response.peers.0.iter() {
            //     let mut peer = Peer::new(*PEER_ID, *peer);
            //     let Ok(framed) = peer.shake_hands_get_framed(info_hash).await else {
            //         continue;
            //     };

            //     let file_loader = file_loader.clone();
            //     let has_rx = has_tx.subscribe();
            //     tokio::spawn(async move {
            //         let _ = peer.event_loop(framed, file_loader, has_rx).await;
            //     });
            // }

            // std::thread::sleep(std::time::Duration::MAX);

            // let torrent = read_torrent(torrent)?;
            // assert!(*piece_i < torrent.info.pieces.0.len() as u32); // piece starts at 0
            // let all_blocks = download_piece(&torrent, *piece_i).await?;

            // let mut file = std::fs::File::create(output).context("create downloaded file")?;
            // file.write_all(&all_blocks)
            //     .context("write downloaded file")?;
        }
        DecodeMetadataType::Download {
            output,
            torrent: torrent_path,
        } => {
            let (req_manager_tx, req_manager_rx) = mpsc::channel(64);

            let req_manager =
                PeerManager::init(req_manager_rx, output.clone(), torrent_path.clone()).await?;

            let torrent = &req_manager.torrent;

            let info_hash = torrent.info_hash();
            let tracker = TrackerRequest::new(&info_hash, PEER_ID, PEER_PORT, torrent.get_length());
            let response = tracker.get_response(&torrent.announce).await?;

            tokio::spawn(async move {
                let _ = req_manager.run().await;
            });

            for &addr in response.peers.0.iter() {
                let req_manager_tx = req_manager_tx.clone();
                tokio::spawn(async move {
                    let peer = Peer::connect_from_addr(addr, info_hash, *PEER_ID, req_manager_tx)
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
                    Peer::connect_from_stream(stream, info_hash, *PEER_ID, req_manager_tx.clone())
                        .await
                        .context("initializing incoming peer connection")
                        .unwrap();
                peer.run().await.unwrap();
            }
        }
    }

    Ok(())
}
