use http::{header, HeaderMap, StatusCode, Uri};
use log::info;
use std::net::SocketAddr;

pub struct RoutedInfo<'a> {
    pub status_code: &'a StatusCode,
    pub path: &'a Uri,
    pub url: &'a str,
    pub user_agent: &'a str,
    pub client_ip: String,
}

impl<'a> RoutedInfo<'a> {
    pub fn new(
        status_code: &'a StatusCode,
        path: &'a Uri,
        url: &'a str,
        req_headers: &'a HeaderMap,
        conn_addr: SocketAddr,
    ) -> Self {
        let user_agent = req_headers
            .get(header::USER_AGENT)
            .map(|v| v.to_str().unwrap_or_default())
            .unwrap_or_default();

        let from_header = if let Some(v) = req_headers.get("X-Forwarded-For") {
            if let Ok(v) = v.to_str() {
                v.split(",").next()
            } else {
                None
            }
        } else {
            None
        };

        let client_ip = if let Some(client_ip) = from_header {
            client_ip.to_owned()
        } else {
            conn_addr.ip().to_string()
        };

        RoutedInfo {
            status_code,
            path,
            url,
            user_agent,
            client_ip,
        }
    }

    pub fn print_log(&self) {
        info!(
            "{} \"{}\" => \"{}\" [Client {}] \"{}\"",
            self.status_code, self.path, self.url, self.client_ip, self.user_agent
        );
    }
}
