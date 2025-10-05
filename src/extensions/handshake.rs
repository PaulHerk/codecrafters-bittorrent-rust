use std::{collections::HashMap, default};

use serde::{Deserialize, Serialize};
use serde_bytes::ByteArray;

use crate::extensions::ExtensionType;

#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum YourIp {
    V4(ByteArray<4>),
    V6(ByteArray<16>),
}

#[derive(Debug, Deserialize, Serialize)]
pub struct HandshakeExtension {
    pub m: HashMap<String, usize>,
    #[serde(flatten)]
    other: Other,
}

#[derive(Debug, Deserialize, Serialize, Default)]
struct Other {
    metadata_size: Option<usize>,
    /// Local TCP listen port.
    p: Option<u16>,
    /// Client name and version (as a utf-8 string).
    v: Option<String>,
    /// A string containing the compact representation of the ip address this peer sees you
    yourip: Option<YourIp>,
    // /// If this peer has an IPv6 interface, this is the compact representation of that address.
    // ipv6: Option<[u8; 16]>,
    // /// If this peer has an IPv4 interface, this is the compact representation of that address.
    // ipv4: Option<[u8; 4]>,
    // /// An integer, the number of outstanding request messages this client supports without dropping any.
    // reqq: Option<u8>,
}

impl HandshakeExtension {
    pub fn new() -> Self {
        let mut m = HashMap::new();
        for &ext in crate::extensions::ACTIVE_EXTENSIONS {
            m.insert(format!("{ext:?}"), ext as usize);
        }
        Self {
            m,
            other: Other::default(),
        }
    }
}
