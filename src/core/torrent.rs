use std::{collections::HashMap, path::PathBuf};

pub use hashes::Hashes;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InfoHash(pub [u8; 20]);

mod ser_info_hash {
    use serde::{Serialize, Serializer};

    use super::InfoHash;

    impl Serialize for InfoHash {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            serializer.serialize_bytes(&self.0)
        }
    }
}

mod hashes {
    use serde::de::{self, Visitor};
    use serde::ser::{Serialize, Serializer};
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
                Err(de::Error::custom(format!(
                    "Bytes which length is a multiple of 20. Got {:?}",
                    v.len()
                )))
            } else {
                Ok(Hashes(
                    v.chunks_exact(20)
                        .map(|slice_20| slice_20.try_into().expect("slice_20 is 20 bytes"))
                        .collect(),
                ))
            }
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
pub struct Torrent {
    /// The url of the tracker.
    pub announce: url::Url,
    /// This maps to a dictionary.
    pub info: Metainfo,
}

impl Torrent {
    pub fn read_from_file(path: &PathBuf) -> Result<Self, TorrentError> {
        let bytes = std::fs::read(path).map_err(|error| TorrentError::IOReadError {
            error,
            path: path.clone(),
        })?;
        let torrent = serde_bencode::from_bytes::<Torrent>(&bytes)?;

        Ok(torrent)
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Metainfo {
    /// The name key maps to a UTF-8 encoded string.
    /// In the single file case, the name key is the name of a file, in the muliple file case,
    /// it's the name of a directory.
    pub name: String,
    /// `piece length` maps to the number of bytes in each piece the file is split into.
    #[serde(rename = "piece length")]
    pub piece_length: u32,
    /// pieces is to be subdivided into strings of length 20,
    /// each of which is the SHA1 hash of the piece at the corresponding index.
    pub pieces: Hashes,
    /// If length is present then the download represents a single file,
    /// otherwise it represents a set of files which go in a directory structure.
    length: Option<u32>,
    #[serde(flatten)]
    pub files: Key,
    #[serde(flatten)]
    pub other: serde_bencode::value::Value,
}

impl Metainfo {
    pub fn info_hash(&self) -> InfoHash {
        let mut hasher = Sha1::new();
        let bytes = serde_bencode::to_bytes(&self)
            .expect("If this doesn't work, the &self provided would be invalid");
        hasher.update(&bytes);
        let info_hash = hasher.finalize();
        InfoHash(info_hash.into())
    }

    pub fn get_length(&self) -> u32 {
        if let Some(length) = self.length {
            length
        } else {
            dbg!(&self);
            todo!("Didn't implement Multifile yet.")
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Key {
    /// In the single file case, length maps to the length of the file in bytes.
    SingleFile {
        /* length: u32, */ md5sum: Option<String>,
    },
    /// For the purposes of the other keys, the multi-file case is treated as only having
    /// a single file by concatenating the files in the order they appear in the files list.
    MultiFile {
        // length: u32,
        files: Vec<File>,
        md5sum: Option<String>,
    },
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct File {
    /// The length of the file, in bytes.
    pub length: u32,
    /// A list of UTF-8 encoded strings corresponding to subdirectory names,
    /// the last of which is the actual file name (a zero length list is an error case).
    pub path: Vec<String>,
}

#[derive(Error, Debug)]
pub enum TorrentError {
    #[error("Failed with error `{error}` to read file with path `{path}`")]
    IOReadError {
        error: std::io::Error,
        path: PathBuf,
    },
    #[error("Failed to deserialize the torrent bencode: `{0}`")]
    InvalidBencode(#[from] serde_bencode::Error),
}
