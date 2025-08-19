use anyhow::Context;
use clap::{Parser, Subcommand};
use codecrafters_bittorrent::peer::Connection;
use codecrafters_bittorrent::torrent::{Key, Torrent};
use codecrafters_bittorrent::tracker::{serialize_info_hash, TrackerRequest, TrackerResponse};
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
            let bytes = std::fs::read(torrent).context("read torrent file")?;
            let torrent = serde_bencode::from_bytes::<Torrent>(&bytes).context("decode torrent")?;
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
            let bytes = std::fs::read(torrent).context("read torrent file")?;
            let torrent = serde_bencode::from_bytes::<Torrent>(&bytes).context("decode torrent")?;
            let Key::SingleFile { length } = torrent.info.files else {
                todo!();
            };
            let request = TrackerRequest::new(PEER_ID, 6881, length);

            let request_parsed = &serde_urlencoded::to_string(request).context("encode request")?;
            let url = format!(
                "{}?info_hash={}&{}",
                torrent.announce,
                &serialize_info_hash(&torrent.info_hash()?),
                request_parsed,
            );
            // !TODO: parse url type-safe, rn this is a workaround because the serializer parses '%' to "%25"
            let response = reqwest::get(url).await.context("send request")?;
            let response_bytes = response
                .bytes()
                .await
                .context("get tracker response bytes")?;

            let response = serde_bencode::from_bytes::<TrackerResponse>(&response_bytes)
                .context("deserialize tracker response")?;
            for peer in response.peers.0 {
                println!("{}", peer);
            }
        }
        DecodeMetadataType::Handshake { torrent, addr } => {
            let bytes = std::fs::read(torrent).context("read torrent file")?;
            let torrent = serde_bencode::from_bytes::<Torrent>(&bytes).context("decode torrent")?;
            let connection = Connection::new(&torrent.info_hash()?, &PEER_ID, *addr)
                .await
                .context("connect to peer")?;
            println!("Peer Id: {}", hex::encode(connection.peer_id_other));
        }
    }
    Ok(())
}
