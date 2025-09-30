use serde::Deserialize;
use thiserror::Error;

use crate::messages::extensions::magnet_links::des_info_hash::InfoHash;

mod des_info_hash {
    use std::fmt;

    use serde::{
        Deserialize, Deserializer,
        de::{self, Visitor},
    };
    use sha1::{Digest, Sha1};

    #[derive(Debug, PartialEq)]
    pub(super) struct InfoHash(pub(super) [u8; 20]);
    struct InfoHashVisitor;

    impl<'de> Visitor<'de> for InfoHashVisitor {
        type Value = InfoHash;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("'urn:btih': followed by the 40-char hex-encoded info hash")
        }

        /* fn visit_bytes<E>(self, v: &[u8]) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            let split = v
            let hex: Vec<char> = v[..40].iter().map(|b| *b as char).collect();
            if hex.len() != 40 {
                return Err(E::invalid_length(hex.len(), 40));
            } else {

            }
        } */
        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            let starting_str_exp = "urn:btih:";
            let starting_str_occ = &v[..starting_str_exp.len()];
            if starting_str_exp != starting_str_occ {
                return Err(E::custom(format!(
                    "invalid prefix, got: {starting_str_occ}, expected: {starting_str_exp}"
                )));
            } else {
                let hex_hash = &v[starting_str_exp.len()..];
                let mut hasher = Sha1::new();
                hasher.update(hex_hash);
                let bytes_hash = hasher.finalize().into();
                Ok(InfoHash(bytes_hash))
            }
        }

        // Similar for other methods:
        //   - visit_i16
        //   - visit_u8
        //   - visit_u16
        //   - visit_u32
        //   - visit_u64
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

#[derive(Deserialize, Debug)]
struct MagnetLink {
    #[serde(rename = "xt")]
    info_hash: InfoHash,
    #[serde(rename = "dn")]
    file_name: Option<String>,
    #[serde(rename = "tr")]
    announce: Option<url::Url>,
}

impl MagnetLink {
    pub(crate) fn from_url(url: &str) -> Result<Self, MagnetLinkError> {
        let url = url::Url::parse(url)?;
        if url.scheme() != "magnet" {
            return Err(MagnetLinkError::NoMagnetLink);
        }
        let query = url.query().ok_or(MagnetLinkError::NoQueryFound)?;
        let magnet_link = serde_urlencoded::from_str(query)?;
        Ok(magnet_link)
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
}

#[cfg(test)]
mod test_magnetlink {
    use super::*;
    #[test]
    fn parse() {
        let magnet_link = MagnetLink::from_url(
            "magnet:?xt=urn:btih:ad42ce8109f54c99613ce38f9b4d87e70f24a165&dn=magnet1.gif&tr=http%3A%2F%2Fbittorrent-test-tracker.codecrafters.io%2Fannounce",
        ).expect("is valid");
        assert_eq!(
            magnet_link.info_hash,
            InfoHash([
                145, 138, 121, 205, 131, 166, 85, 159, 56, 46, 90, 244, 198, 29, 162, 146, 38, 98,
                61, 39
            ])
        );
        assert_eq!(
            dbg!(magnet_link.announce),
            Some(
                url::Url::parse("http://bittorrent-test-tracker.codecrafters.io/announce")
                    .expect("is valid")
            )
        );
        assert_eq!(magnet_link.file_name, Some("magnet1.gif".to_owned()));
    }
}
