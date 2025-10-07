use crate::magnet_links::metadata_requester::MetadataRequester;

use super::ExtensionHandler;

pub struct ExtensionFactory;

impl ExtensionFactory {
    pub fn build(name: &str) -> Option<Box<dyn ExtensionHandler>> {
        match name {
            "ut_metadata" => Some(Box::new(MetadataRequester::new())),
            // "ut_pex" => Some(Box::new(PexHandler::new())), // Example for another extension
            _ => None, // This extension is not supported by our client
        }
    }
}
