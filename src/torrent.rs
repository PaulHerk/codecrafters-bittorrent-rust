use anyhow::Context;
pub use hashes::Hashes;
use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

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
pub struct Torrent {
    /// The url of the tracker.
    pub announce: String,
    /// This maps to a dictionary.
    pub info: Info,
}

impl Torrent {
    pub fn info_hash(&self) -> Result<[u8; 20], anyhow::Error> {
        let mut hasher = Sha1::new();
        hasher.update(serde_bencode::to_bytes(&self.info).context("re-encode info")?);
        let info_hash = hasher.finalize();
        info_hash.try_into().context("convert to [u8; 20]")
    }

    pub fn get_length(&self) -> u32 {
        if let Key::SingleFile { length } = self.info.files {
            length
        } else {
            todo!()
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Info {
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
    #[serde(flatten)]
    pub files: Key,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Key {
    /// In the single file case, length maps to the length of the file in bytes.
    SingleFile { length: u32 },
    /// For the purposes of the other keys, the multi-file case is treated as only having
    /// a single file by concatenating the files in the order they appear in the files list.
    MultiFile { files: Vec<File> },
}
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct File {
    /// The length of the file, in bytes.
    pub length: u32,
    /// A list of UTF-8 encoded strings corresponding to subdirectory names,
    /// the last of which is the actual file name (a zero length list is an error case).
    pub path: Vec<String>,
}
