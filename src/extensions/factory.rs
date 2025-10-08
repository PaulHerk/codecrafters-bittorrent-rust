use std::str::FromStr;

use crate::{extensions::ExtensionType, magnet_links::metadata_requester::MetadataRequester};

use super::ExtensionHandler;

pub struct ExtensionFactory;

impl ExtensionFactory {
    pub fn build(name: &str) -> Option<Box<dyn ExtensionHandler>> {
        match ExtensionType::from_str(name).ok()? {
            ExtensionType::Metadata => Some(Box::new(MetadataRequester::new())),
            _ => None,
        }
    }
}
