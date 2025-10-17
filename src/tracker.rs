use bytes::Bytes;
use futures_util::future::select_ok;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{torrent::InfoHash, tracker::peers::PeerConnections};

#[derive(Debug, Clone, Serialize)]
pub struct TrackerRequest<'a> {
    /// the info hash of the torrent
    info_hash: &'a InfoHash,
    /// a unique identifier for your client
    peer_id: &'a [u8; 20],
    /// the port your client is listening on
    port: u16,
    /// the total amount uploaded so far
    uploaded: u32,
    /// the total amount downloaded so far
    downloaded: u32,
    /// the number of bytes left to download
    left: u32,
    /// whether the peer list should use the compact representation
    /// The compact representation is more commonly used in the wild, the non-compact representation is mostly supported for backward-compatibility.
    compact: u8,
}

impl<'a> TrackerRequest<'a> {
    pub fn new(
        info_hash: &'a InfoHash,
        peer_id: &'a [u8; 20],
        port: u16,
        file_length: u32,
    ) -> Self {
        Self {
            info_hash,
            peer_id,
            port,
            uploaded: 0,
            downloaded: 0,
            left: file_length,
            compact: 1, // TODO
        }
    }
    fn to_url_encoded(&self) -> String {
        let mut url_encoded = String::new();
        url_encoded.push_str(&format!(
            "info_hash={}",
            escape_bytes_url(&self.info_hash.0)
        ));
        url_encoded.push_str(&format!("&peer_id={}", escape_bytes_url(self.peer_id)));
        url_encoded.push_str(&format!("&port={}", self.port));
        url_encoded.push_str(&format!("&uploaded={}", self.uploaded));
        url_encoded.push_str(&format!("&downloaded={}", self.downloaded));
        url_encoded.push_str(&format!("&left={}", self.left));
        url_encoded.push_str(&format!("&compact={}", self.compact));
        url_encoded
    }

    pub async fn get_response(
        &self,
        announce_urls: impl IntoIterator<Item = url::Url>,
    ) -> Result<TrackerResponse, TrackerRequestError> {
        let mut request_list = Vec::new();
        let mut url_list = Vec::new();

        let client = reqwest::Client::builder()
        .user_agent(
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:142.0) Gecko/20100101 Firefox/142.0",
        )
        .build()?;
        for mut url in announce_urls {
            url.set_query(Some(&self.to_url_encoded()));
            url_list.push(url.clone());
            request_list.push(client.get(url).send());
        }
        let (response, _rem) = select_ok(request_list).await?;
        let url = response.url().clone();
        let response_bytes = Bytes::copy_from_slice(&response.bytes().await?);

        serde_bencode::from_bytes::<TrackerResponse>(&response_bytes).map_err(|des_err| {
            TrackerRequestError::InvalidResponse {
                error: des_err,
                response: response_bytes,
                url: url.to_string(),
            }
        })
    }
}

fn escape_bytes_url(bytes: &[u8; 20]) -> String {
    bytes
        .iter()
        .map(|b| {
            if b.is_ascii_alphanumeric() {
                (*b as char).to_string()
            } else {
                format!("%{}", hex::encode([*b]))
            }
        })
        .collect()
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TrackerResponse {
    /// An integer, indicating how often your client should make a request to the tracker, in seconds.
    pub interval: usize,
    /// A string, which contains list of peers that your client can connect to.
    /// Each peer is represented using 6 bytes.
    /// The first 4 bytes are the peer's IP address and the last 2 bytes are the peer's port number.
    pub peers: PeerConnections,
}

mod peers {
    use std::{
        fmt,
        net::{Ipv4Addr, SocketAddrV4},
    };

    use serde::{
        Deserialize, Deserializer, Serialize, Serializer,
        de::{self, Visitor},
    };
    #[derive(Debug, Clone)]
    pub struct PeerConnections(pub Vec<SocketAddrV4>);
    struct PeersVisitor;

    impl Serialize for PeerConnections {
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
        type Value = PeerConnections;

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
            Ok(PeerConnections(
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

    impl<'de> Deserialize<'de> for PeerConnections {
        fn deserialize<D>(deserializer: D) -> Result<PeerConnections, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_bytes(PeersVisitor)
        }
    }
}

// #[derive(Debug, Clone, Deserialize, Serialize)]
// pub struct PeerConnection {
//     #[serde(rename = "peer id")]
//     pub peer_id: Option<serde_bytes::ByteArray<20>>,
//     #[serde(with = "serde_bytes")]
//     ip: Vec<u8>,
//     port: u16,
// }

// impl PeerConnection {
//     pub fn get_socket_addr(&self) -> SocketAddrV4 {
//         if let Ok(ip) = String::from_utf8(self.ip.clone()) {
//             SocketAddrV4::new(ip.parse().unwrap(), self.port)
//         } else {
//             todo!(); // impl Ipv6
//         }
//         // TODO: error handling (IP address or dns name as a string)
//     }
// }

#[derive(Error, Debug)]
pub enum TrackerRequestError {
    #[error("Failed to parse announce url: `{0}`")]
    InvalidUrl(#[from] url::ParseError),
    #[error("Failed with error: `{error}` to deserialize tracker response: `{response:?}`")]
    InvalidResponse {
        error: serde_bencode::Error,
        response: bytes::Bytes,
        url: String,
    },
    #[error("Something failed with requesting the tracker-response: `{0}`")]
    ReqwestError(#[from] reqwest::Error),
}
