use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_bytes::ByteArray;

#[derive(Debug, Deserialize, Serialize, Clone, Copy)]
#[serde(untagged)]
pub enum YourIp {
    /// If this peer has an IPv4 interface, this is the compact representation of that address.
    V4(ByteArray<4>),
    /// If this peer has an IPv6 interface, this is the compact representation of that address.
    V6(ByteArray<16>),
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct HandshakeExtension {
    pub(crate) m: HashMap<String, u8>,
    #[serde(flatten)]
    pub(crate) other: AdditionalHandshakeInfo,
}

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub(crate) struct AdditionalHandshakeInfo {
    pub(crate) metadata_size: Option<usize>,
    /// Local TCP listen port.
    pub(crate) p: Option<u16>,
    /// Client name and version (as a utf-8 string).
    pub(crate) v: Option<String>,
    /// A string containing the compact representation of the ip address this peer sees you
    pub(crate) yourip: Option<YourIp>,
    /// An integer, the number of outstanding request messages this client supports without dropping any.
    pub(crate) reqq: Option<usize>,
}

impl HandshakeExtension {
    pub fn new() -> Self {
        let mut m = HashMap::new();
        for &ext in crate::extensions::ACTIVE_EXTENSIONS {
            m.insert(ext.to_string(), ext as u8);
        }
        Self {
            m,
            other: AdditionalHandshakeInfo::default(),
        }
    }
}
