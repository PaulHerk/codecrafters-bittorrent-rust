use anyhow::Context;
use clap::{Parser, Subcommand};
use codecrafters_bittorrent::*;
use reqwest::Url;
use serde_bencode;
use std::net::SocketAddrV4;
use std::path::PathBuf;

const PEER_ID: &[u8; 20] = b"00112233445566778899";

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
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // You can check for the existence of subcommands, and if found use their
    // matches just as you would the top level cmd
    match &cli.command {
        DecodeMetadataType::Decode { value } => {
            let decoded_value: serde_bencode::value::Value =
                serde_bencode::from_str(value).context("decode bencode")?;
            println!("{:?}", decoded_value);
        }
        DecodeMetadataType::Info { torrent } => {
            let torrent = read_torrent(&torrent)?;
            println!("Tracker URL: {}", torrent.announce);
            if let Key::SingleFile { length } = torrent.info.files {
                println!("Length: {}", length);
            }
            let info_hash = torrent.info_hash()?;
            println!("Info Hash: {}", hex::encode(info_hash));
            println!("Piece Length: {}", torrent.info.piece_length);
            println!("Piece Hashes:",);
            for hash in torrent.info.pieces.0.iter() {
                println!("{}", hex::encode(hash));
            }
        }
        DecodeMetadataType::Peers { torrent } => {
            let torrent = read_torrent(torrent)?;
            let length = get_length(&torrent);
            let response = get_response(&torrent, length).await?;
            for peer in response.peers {
                println!("{:?}", peer);
            }
        }
        DecodeMetadataType::Handshake { torrent, addr } => {
            let torrent = read_torrent(torrent)?;
            let mut peer = Peer::new(*PEER_ID, *addr);
            let _framed = peer
                .shake_hands_get_framed(torrent.info_hash()?)
                .await
                .context("shake hands")?;
            println!("Peer Id: {}", hex::encode(peer.peer_id));
        }
        DecodeMetadataType::DownloadPiece {
            output: _,
            torrent: _,
            piece: _piece_i,
        } => {
            // let torrent = read_torrent(torrent)?;
            // assert!(*piece_i < torrent.info.pieces.0.len() as u32); // piece starts at 0
            // let all_blocks = download_piece(&torrent, *piece_i).await?;

            // let mut file = std::fs::File::create(output).context("create downloaded file")?;
            // file.write_all(&all_blocks)
            //     .context("write downloaded file")?;
        }
        DecodeMetadataType::Download { output, torrent } => {
            let torrent = read_torrent(torrent)?;
            let output = match output {
                Some(output) => output,
                None => &PathBuf::from(&torrent.info.name),
            };
            let length = torrent.get_length();
            let response = get_response(&torrent, length).await?;

            let info_hash = torrent.info_hash()?;
            let peer_data = PeerData::new(torrent, &[]);
            let (tx, rx) = tokio::sync::broadcast::channel(10);
            for peer in response.peers.iter() {
                let mut peer = Peer::new(*PEER_ID, peer.get_socket_addr());
                let framed = peer
                    .shake_hands_get_framed(info_hash)
                    .await
                    .context("shake hands")?;

                let peer_data = peer_data.clone();
                let rx = tx.subscribe();
                let tx = tx.clone();
                tokio::spawn(async move {
                    peer.event_loop(framed, peer_data, (tx, rx)).await.unwrap();
                });
            }

            let file = std::fs::File::create(output).context("create downloaded file")?;
            peer_data.write_file(file, rx).await.context("write file")?;

            loop {}
        }
    }

    Ok(())
}

fn read_torrent(torrent: &PathBuf) -> anyhow::Result<Torrent> {
    let bytes = std::fs::read(torrent).context("read torrent file")?;
    let torrent = serde_bencode::from_bytes::<Torrent>(&bytes).context("decode torrent")?;
    Ok(torrent)
}

fn get_length(torrent: &Torrent) -> u32 {
    let Key::SingleFile { length } = torrent.info.files else {
        todo!();
    };
    length
}

async fn get_response(torrent: &Torrent, length: u32) -> anyhow::Result<TrackerResponse> {
    let request = TrackerRequest::new(torrent.info_hash()?, *PEER_ID, 6881, length);

    let mut url = Url::parse(&torrent.announce).context("parse url")?;
    url.set_query(Some(&request.to_url_encoded()));

    let response = reqwest::get(url).await.context("send request")?;
    let response_bytes = response
        .bytes()
        .await
        .context("get tracker response bytes")?;

    serde_bencode::from_bytes::<TrackerResponse>(&response_bytes)
        .context("deserialize tracker response")
}
