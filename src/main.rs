use anyhow::Context;
use clap::{Parser, Subcommand};
use codecrafters_bittorrent::*;
use reqwest::Url;
use serde_bencode;
use sha1::{Digest, Sha1};
use std::io::Write;
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
            output,
            torrent,
            piece: piece_i,
        } => {
            let torrent = read_torrent(torrent)?;
            assert!(*piece_i < torrent.info.pieces.0.len() as u32); // piece starts at 0
            let all_blocks = download_piece(&torrent, *piece_i).await?;

            let mut file = std::fs::File::create(output).context("create downloaded file")?;
            file.write_all(&all_blocks)
                .context("write downloaded file")?;
        }
        DecodeMetadataType::Download { output, torrent } => {
            let torrent = read_torrent(torrent)?;
            let output = match output {
                Some(output) => output,
                None => &PathBuf::from(&torrent.info.name),
            };
            let mut file = std::fs::File::create(output).context("create downloaded file")?;
            println!("Downloading {} pieces", torrent.info.pieces.0.len());
            for piece_i in 0..torrent.info.pieces.0.len() as u32 {
                let all_blocks = download_piece(&torrent, piece_i).await?;
                file.write_all(&all_blocks)
                    .context("write downloaded file")?;
            }
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

async fn download_piece(torrent: &Torrent, piece_i: u32) -> anyhow::Result<Vec<u8>> {
    let length = get_length(&torrent);
    let response = get_response(&torrent, length).await?;
    let mut peer = Peer::new(
        // response.peers[1].peer_id.into_array(),
        [0; 20],
        response.peers[1].get_socket_addr(),
    );

    let framed = peer
        .shake_hands_get_framed(torrent.info_hash()?)
        .await
        .context("shake hands")?;

    let mut connection = Connection::init_data_exchange(framed)
        .await
        .context("init")?;
    let piece_hash = torrent
        .info
        .pieces
        .0
        .get(piece_i as usize)
        .context("piece not found")?;
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

    let mut all_blocks = vec![0_u8; piece_size as usize];
    for block in 0..nblocks {
        let block_length = if block == nblocks - 1 && piece_size % BLOCK_MAX != 0 {
            piece_size % BLOCK_MAX
        } else {
            BLOCK_MAX
        };
        let request_payload = RequestPiecePayload::new(piece_i, block * BLOCK_MAX, block_length);
        let response = connection
            .get_block(request_payload)
            .await
            .context(format!("download piece {}", block))?;
        assert_eq!(response.index, piece_i);

        all_blocks[response.begin as usize..block_length as usize + response.begin as usize]
            .copy_from_slice(&response.block[..block_length as usize]);
    }

    let mut sha1 = Sha1::new();
    sha1.update(&all_blocks);
    let hash: [u8; 20] = sha1
        .finalize()
        .try_into()
        .expect("GenericArray<_, 20> == [u8; 20]");
    assert_eq!(piece_hash, &hash);

    Ok(all_blocks)
}
