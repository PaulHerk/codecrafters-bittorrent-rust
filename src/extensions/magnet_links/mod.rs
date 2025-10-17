use std::{net::SocketAddrV4, str::FromStr};

use thiserror::Error;
use url::form_urlencoded::Parse;

use crate::torrent::InfoHash;

// mod before_download_manager;
// mod peer_manager_init;
pub(crate) mod metadata_msg;
pub(crate) mod metadata_piece_manager;

const INFO_HASH_PREFIX: &'static str = "urn:btih";

impl FromStr for InfoHash {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let starting_str_occ = &s[..INFO_HASH_PREFIX.len()];
        if INFO_HASH_PREFIX != starting_str_occ {
            anyhow::bail!("invalid prefix, got: {starting_str_occ}, expected: {INFO_HASH_PREFIX}")
        } else {
            let hex_hash = &s[INFO_HASH_PREFIX.len() + 1..]; // +1 for the colon
            let bytes_hash = hex::decode(hex_hash).map_err(|e| {
                anyhow::anyhow!("`{hex_hash}` is not a valid hex string. Failed with error: {e}")
            })?;
            let bytes_hash: [u8; 20] = bytes_hash.try_into().map_err(|e| {
                anyhow::anyhow!("Couldn't convert the hex into a valid 20 byte array: `{e:?}`")
            })?;
            Ok(InfoHash(bytes_hash))
        }
    }
}

#[derive(Debug, Clone)]
pub struct MagnetLink {
    pub info_hash: InfoHash,
    file_name: Option<String>,
    trackers: Vec<url::Url>,
    peer_addrs: Vec<SocketAddrV4>,
}

impl MagnetLink {
    pub fn from_url(url: &str) -> Result<Self, MagnetLinkError> {
        let url = url::Url::parse(url)?;
        if url.scheme() != "magnet" {
            return Err(MagnetLinkError::NoMagnetLink);
        }

        Self::from_query_pairs(url.query_pairs())
    }

    pub fn get_announce_url(&self) -> Result<url::Url, MagnetLinkError> {
        let url = self
            .trackers
            .first()
            .ok_or(MagnetLinkError::NoTrackerUrl)?
            .clone();
        Ok(url)
    }

    fn from_query_pairs(pairs: Parse) -> Result<Self, MagnetLinkError> {
        let mut trackers = Vec::new();
        let mut peer_addrs = Vec::new();
        let mut file_name = None;
        let mut info_hash = None;

        for (key, value) in pairs.into_iter() {
            match key.as_ref() {
                "xt" => {
                    info_hash = Some(InfoHash::from_str(&value)?);
                }
                "tr" => {
                    if let Ok(mut url) = url::Url::parse(&value) {
                        // current workaround for some trackers that only announce their udp addr but also have support for http
                        if url.scheme() == "udp" {
                            url.set_path("/announce");
                            let url_str = &url.as_str()[3..];
                            url = url::Url::parse(&format!("http{url_str}")).expect("is valid");
                            // the reason I do this is because I cannot set the scheme via .set_scheme() from udp to http
                        }
                        trackers.push(url);
                    }
                }
                "x.pe" => {
                    if let Ok(addr) = SocketAddrV4::from_str(&value) {
                        peer_addrs.push(addr);
                    }
                }
                "dn" => {
                    if !value.is_empty() {
                        file_name = Some(value.into_owned())
                    }
                }
                _ => {}
            }
        }

        let info_hash = info_hash.ok_or(MagnetLinkError::InvalidInfoHash(anyhow::anyhow!(
            "No info hash provided."
        )))?;
        Ok(Self {
            info_hash,
            trackers,
            peer_addrs,
            file_name,
        })
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
    #[error("Failed to deserialize the info-hash")]
    InvalidInfoHash(#[from] anyhow::Error),
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
            "magnet:?xt=urn:btih:ad42ce8109f54c99613ce38f9b4d87e70f24a165&dn=magnet1.gif&tr=http%3A%2F%2Fbittorrent-test-tracker.codecrafters.io%2Fannounce&tr=udp%3A%2F%2Fbittorrent-test-tracker.codecrafters.io"
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
            vec![
                url::Url::parse("http://bittorrent-test-tracker.codecrafters.io/announce")
                    .expect("is valid");
                2
            ]
        );
        assert_eq!(magnet_link.file_name, Some("magnet1.gif".to_owned()));
    }
}
