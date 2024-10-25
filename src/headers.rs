use crate::vars;
use http::{header, HeaderMap};

pub fn build_from_request(source_headers: &HeaderMap) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for (key, value) in source_headers.iter() {
        let value = if key == header::HOST {
            vars::upstream_domain().clone()
        } else {
            value.clone()
        };

        headers.insert(key, value.clone());
    }

    headers
}

pub trait AppendHeaders {
    fn append_headers(self, headers: &HeaderMap) -> Self;
}

// Ignore the response headers that should not be forwarded
const IGNORE_RESPONSE_HEADERS: [header::HeaderName; 6] = [
    header::CONNECTION,        // Keep-Alive is not supported
    header::CONTENT_LENGTH,    // The page has been modified
    header::CONTENT_ENCODING,  // The page has been modified
    header::ETAG,              // The page has been modified
    header::LAST_MODIFIED,     // The page has been modified
    header::TRANSFER_ENCODING, // Determine by proxy server
];

impl AppendHeaders for http::response::Builder {
    fn append_headers(self, headers: &HeaderMap) -> Self {
        headers.iter().fold(self, |builder, (key, value)| {
            if !IGNORE_RESPONSE_HEADERS.contains(key) {
                builder.header(key, value)
            } else {
                builder
            }
        })
    }
}
