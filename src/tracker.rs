use serde::{Deserialize, Serialize};

use crate::tracker::peers::Peers;

pub fn serialize_info_hash(info_hash: &[u8; 20]) -> String {
    info_hash
        .iter()
        .map(|b| format!("%{}", hex::encode([*b])))
        .collect()
}

/// NOTE THAT THE INFO_HASH IS NOT INCLUDED IN THE REQUEST
#[derive(Debug, Clone, Serialize)]
pub struct TrackerRequest {
    /// the info hash of the torrent
    // #[serde(serialize_with = "serialize_info_hash")]
    // pub info_hash: [u8; 20],
    /// a unique identifier for your client
    pub peer_id: String,
    /// the port your client is listening on
    pub port: u16,
    /// the total amount uploaded so far
    pub uploaded: usize,
    /// the total amount downloaded so far
    pub downloaded: usize,
    /// the number of bytes left to download
    pub left: usize,
    /// whether the peer list should use the compact representation
    /// The compact representation is more commonly used in the wild, the non-compact representation is mostly supported for backward-compatibility.
    pub compact: u8,
}

impl TrackerRequest {
    pub fn new(peer_id: String, port: u16, file_length: usize) -> Self {
        if peer_id.len() > 20 {
            panic!(
                "peer_id must be less than 20 bytes, got {peer_id} with len {}",
                peer_id.len()
            );
        }
        Self {
            peer_id,
            port,
            uploaded: 0,
            downloaded: 0,
            left: file_length,
            compact: 1,
        }
    }
}

mod peers {
    use std::{
        fmt,
        net::{Ipv4Addr, SocketAddrV4},
    };

    use serde::{
        de::{self, Visitor},
        Deserialize, Deserializer, Serialize, Serializer,
    };
    #[derive(Debug, Clone)]
    pub struct Peers(pub Vec<SocketAddrV4>);
    struct PeersVisitor;

    impl Serialize for Peers {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let mut bytes = Vec::with_capacity(self.0.len() * 6);
            for peer in &self.0 {
                bytes.extend(&peer.ip().octets());
                bytes.extend(&peer.port().to_be_bytes());
            }
            serializer.serialize_bytes(&bytes)
        }
    }

    impl<'de> Visitor<'de> for PeersVisitor {
        type Value = Peers;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("A string of multiples of 6 bytes")
        }
        fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            if v.len() % 6 != 0 {
                return Err(de::Error::custom(format!(
                    "Bytes which length is a multiple of 6. Got {:?}",
                    v.len()
                )));
            }
            Ok(Peers(
                v.chunks_exact(6)
                    .map(|chunk| {
                        if let &[a, b, c, d, p1, p2] = chunk {
                            SocketAddrV4::new(
                                Ipv4Addr::new(a, b, c, d),
                                u16::from_be_bytes([p1, p2]),
                            )
                        } else {
                            unreachable!();
                        }
                    })
                    .collect(),
            ))
        }
    }

    impl<'de> Deserialize<'de> for Peers {
        fn deserialize<D>(deserializer: D) -> Result<Peers, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_bytes(PeersVisitor)
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct TrackerResponse {
    /// An integer, indicating how often your client should make a request to the tracker, in seconds.
    pub interval: usize,
    /// A string, which contains list of peers that your client can connect to.
    /// Each peer is represented using 6 bytes.
    /// The first 4 bytes are the peer's IP address and the last 2 bytes are the peer's port number.
    pub peers: Peers,
}
