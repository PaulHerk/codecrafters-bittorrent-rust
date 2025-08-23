use std::net::SocketAddrV4;

use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize)]
pub struct TrackerRequest {
    /// the info hash of the torrent
    pub info_hash: [u8; 20],
    /// a unique identifier for your client
    pub peer_id: [u8; 20],
    /// the port your client is listening on
    pub port: u16,
    /// the total amount uploaded so far
    pub uploaded: u32,
    /// the total amount downloaded so far
    pub downloaded: u32,
    /// the number of bytes left to download
    pub left: u32,
    /// whether the peer list should use the compact representation
    /// The compact representation is more commonly used in the wild, the non-compact representation is mostly supported for backward-compatibility.
    pub compact: u8,
}

impl TrackerRequest {
    pub fn new(info_hash: [u8; 20], peer_id: [u8; 20], port: u16, file_length: u32) -> Self {
        Self {
            info_hash,
            peer_id,
            port,
            uploaded: 0,
            downloaded: 0,
            left: file_length,
            compact: 0, // TODO
        }
    }
    pub fn to_url_encoded(&self) -> String {
        let mut url_encoded = String::new();
        url_encoded.push_str(&format!("info_hash={}", escape_bytes_url(&self.info_hash)));
        url_encoded.push_str(&format!("&peer_id={}", escape_bytes_url(&self.peer_id)));
        url_encoded.push_str(&format!("&port={}", self.port));
        url_encoded.push_str(&format!("&uploaded={}", self.uploaded));
        url_encoded.push_str(&format!("&downloaded={}", self.downloaded));
        url_encoded.push_str(&format!("&left={}", self.left));
        url_encoded.push_str(&format!("&compact={}", self.compact));
        url_encoded
    }
}

fn escape_bytes_url(bytes: &[u8; 20]) -> String {
    bytes
        .iter()
        .map(|b| format!("%{}", hex::encode([*b])))
        .collect()
}

#[derive(Debug, Clone, Deserialize)]
pub struct TrackerResponse {
    /// An integer, indicating how often your client should make a request to the tracker, in seconds.
    pub interval: usize,
    /// A string, which contains list of peers that your client can connect to.
    /// Each peer is represented using 6 bytes.
    /// The first 4 bytes are the peer's IP address and the last 2 bytes are the peer's port number.
    pub peers: Vec<PeerConnection>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PeerConnection {
    #[serde(rename = "peer id")]
    pub peer_id: serde_bytes::ByteArray<20>,
    #[serde(with = "serde_bytes")]
    ip: Vec<u8>,
    port: u16,
}

impl PeerConnection {
    pub fn get_socket_addr(&self) -> SocketAddrV4 {
        if let Ok(ip) = String::from_utf8(self.ip.clone()) {
            SocketAddrV4::new(ip.parse().unwrap(), self.port)
        } else {
            todo!(); // impl Ipv6
        }
        // TODO: error handling (IP address or dns name as a string)
        // TODO: compact form
    }
}
