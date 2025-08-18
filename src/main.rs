use anyhow::Context;
use clap::{Args, Parser, Subcommand};
use hashes::Hashes;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use serde_bencode;
use sha1::{Digest, Sha1};
use std::{env, path::PathBuf};

mod hashes {
    use serde::de::{self, Visitor};
    use serde::ser::{Serialize, SerializeMap, SerializeSeq, Serializer};
    use serde::{Deserialize, Deserializer};
    use std::fmt;

    #[derive(Debug, Clone)]
    pub struct Hashes(pub Vec<[u8; 20]>);
    struct HashesVisitor;

    impl Serialize for Hashes {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let single_slice = self.0.concat();
            serializer.serialize_bytes(&single_slice)
        }
    }

    impl<'de> Visitor<'de> for HashesVisitor {
        type Value = Hashes;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("Bytes which length is a multiple of 20")
        }

        fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            if v.len() % 20 != 0 {
                return Err(de::Error::custom(format!(
                    "Bytes which length is a multiple of 20. Got {:?}",
                    v.len()
                )));
            }
            Ok(Hashes(
                v.chunks_exact(20)
                    .map(|slice_20| slice_20.try_into().expect("slice_20 is 20 bytes"))
                    .collect(),
            ))
        }
    }

    impl<'de> Deserialize<'de> for Hashes {
        fn deserialize<D>(deserializer: D) -> Result<Hashes, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_bytes(HashesVisitor)
        }
    }
}
#[derive(Debug, Clone, Deserialize, Serialize)]
/// The Metainfo files
struct Torrent {
    /// The url of the tracker.
    announce: String,
    /// This maps to a dictionary.
    info: Info,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Info {
    /// The name key maps to a UTF-8 encoded string.
    /// In the single file case, the name key is the name of a file, in the muliple file case,
    /// it's the name of a directory.
    name: String,
    /// `piece length` maps to the number of bytes in each piece the file is split into.
    #[serde(rename = "piece length")]
    piece_length: usize,
    /// pieces is to be subdivided into strings of length 20,
    /// each of which is the SHA1 hash of the piece at the corresponding index.
    pieces: Hashes,
    /// If length is present then the download represents a single file,
    /// otherwise it represents a set of files which go in a directory structure.
    #[serde(flatten)]
    files: Key,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
enum Key {
    /// In the single file case, length maps to the length of the file in bytes.
    SingleFile { length: usize },
    /// For the purposes of the other keys, the multi-file case is treated as only having
    /// a single file by concatenating the files in the order they appear in the files list.
    MultiFile { files: Vec<File> },
}
#[derive(Debug, Clone, Deserialize, Serialize)]
struct File {
    /// The length of the file, in bytes.
    length: usize,
    /// A list of UTF-8 encoded strings corresponding to subdirectory names,
    /// the last of which is the actual file name (a zero length list is an error case).
    path: Vec<String>,
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: DecodeMetadataType,
}
#[derive(Debug, Subcommand)]
enum DecodeMetadataType {
    Decode { value: String },
    Info { torrent: PathBuf },
}

// Usage: your_program.sh decode "<encoded_value>"
fn main() -> anyhow::Result<()> {
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
            let decoded_value =
                serde_bencode::from_bytes::<Torrent>(&bytes).context("decode torrent")?;
            println!("Tracker URL: {}", decoded_value.announce);
            if let Key::SingleFile { length } = decoded_value.info.files {
                println!("Length: {}", length);
            }
            let info_bytes =
                serde_bencode::to_bytes(&decoded_value.info).context("re-encode info")?;
            let mut hasher = Sha1::new();
            hasher.update(info_bytes);
            let info_hash = hasher.finalize();
            println!("Info Hash: {}", hex::encode(info_hash));
            println!("Piece Length: {}", decoded_value.info.piece_length);
            println!("Piece Hashes:",);
            for hash in decoded_value.info.pieces.0 {
                println!("{}", hex::encode(hash));
            }
        }
    }
    Ok(())
}
