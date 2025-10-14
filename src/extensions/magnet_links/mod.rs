use std::net::SocketAddrV4;

use serde::Deserialize;
use thiserror::Error;

use crate::torrent::InfoHash;

// mod before_download_manager;
// mod peer_manager_init;
pub(crate) mod metadata_msg;
pub(crate) mod metadata_piece_manager;

const INFO_HASH_PREFIX: &'static str = "urn:btih";
mod des_info_hash {
    use std::fmt;

    use serde::{
        Deserialize, Deserializer,
        de::{self, Visitor},
    };

    use crate::{extensions::magnet_links::INFO_HASH_PREFIX, torrent::InfoHash};

    struct InfoHashVisitor;

    impl<'de> Visitor<'de> for InfoHashVisitor {
        type Value = InfoHash;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("'urn:btih': followed by the 40-char hex-encoded info hash")
        }

        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            let starting_str_occ = &v[..INFO_HASH_PREFIX.len()];
            if INFO_HASH_PREFIX != starting_str_occ {
                Err(E::custom(format!(
                    "invalid prefix, got: {starting_str_occ}, expected: {INFO_HASH_PREFIX}"
                )))
            } else {
                let hex_hash = &v[INFO_HASH_PREFIX.len() + 1..]; // +1 for the colon
                let bytes_hash = hex::decode(hex_hash).map_err(|e| {
                    E::custom(format!(
                        "`{hex_hash}` is not a valid hex string. Failed with error: {e}"
                    ))
                })?;
                let bytes_hash: [u8; 20] = bytes_hash.try_into().map_err(|e| {
                    E::custom(format!(
                        "Couldn't convert the hex into a valid 20 byte array: `{e:?}`"
                    ))
                })?;
                Ok(InfoHash(bytes_hash))
            }
        }
    }
    impl<'de> Deserialize<'de> for InfoHash {
        fn deserialize<D>(deserializer: D) -> Result<InfoHash, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_bytes(InfoHashVisitor)
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct MagnetLink {
    #[serde(rename = "xt")]
    pub info_hash: InfoHash,
    #[serde(rename = "dn")]
    file_name: Option<String>,
    #[serde(rename = "tr")]
    trackers: Option<url::Url>,
    #[serde(rename = "x.pe")]
    peer_addr: Option<SocketAddrV4>,
}

impl MagnetLink {
    pub fn from_url(url: &str) -> Result<Self, MagnetLinkError> {
        let url = url::Url::parse(url)?;
        if url.scheme() != "magnet" {
            return Err(MagnetLinkError::NoMagnetLink);
        }
        // TODO: we need to update the deserialialization to use .query_pairs() because apparently serde_urlencoded() doesn't support multiple fields
        let query = url
            .as_str()
            .split_once('?')
            .ok_or(MagnetLinkError::NoQueryFound)?
            .1;
        let magnet_link = serde_urlencoded::from_str(query)?;
        Ok(magnet_link)
    }
    pub fn get_announce_url(&self) -> Result<url::Url, MagnetLinkError> {
        let mut url = self.trackers.clone().ok_or(MagnetLinkError::NoTrackerUrl)?;
        // current workaround for some trackers that only announce their udp addr but also have support for http
        if url.scheme() == "udp" {
            url.set_path("/announce");
            let url_str = &url.as_str()[3..];
            url = url::Url::parse(&format!("http{url_str}")).expect("is valid");
            // the reason I do this is because I cannot set the scheme via .set_scheme() from udp to http
        }
        Ok(url)
    }
}

#[derive(Error, Debug)]
pub enum MagnetLinkError {
    #[error("Failed to parse the provided string to a valid url with the error: `{0}`")]
    InvalidUrl(#[from] url::ParseError),
    #[error("The provided link is no magnet link.")]
    NoMagnetLink,
    #[error("No query was provided.")]
    NoQueryFound,
    #[error("Failed to deserialize the query in the link with the error: `{0}`")]
    FailedToDesQuery(#[from] serde_urlencoded::de::Error),
    #[error("downloading from a magnetlink without a provided tracker url isn't supported")]
    NoTrackerUrl,
}

#[cfg(test)]
mod test_magnetlink {
    use super::*;
    #[test]
    fn parse() {
        let magnet_link = MagnetLink::from_url(
            "magnet:?xt=urn:btih:ad42ce8109f54c99613ce38f9b4d87e70f24a165&dn=magnet1.gif&tr=http%3A%2F%2Fbittorrent-test-tracker.codecrafters.io%2Fannounce"
        ).expect("is valid");
        assert_eq!(
            magnet_link.info_hash,
            InfoHash([
                173, 66, 206, 129, 9, 245, 76, 153, 97, 60, 227, 143, 155, 77, 135, 231, 15, 36,
                161, 101
            ])
        );
        assert_eq!(
            magnet_link.trackers,
            Some(
                url::Url::parse("http://bittorrent-test-tracker.codecrafters.io/announce")
                    .expect("is valid")
            )
        );
        assert_eq!(magnet_link.file_name, Some("magnet1.gif".to_owned()));
    }
}
