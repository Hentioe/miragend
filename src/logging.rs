use crate::vars;
use chrono::Local;
use env_logger::Builder;
use http::{header, HeaderMap, StatusCode, Uri};
use log::{info, Level, LevelFilter};
use std::io::Write;
use std::net::SocketAddr;

pub fn init_logger() {
    Builder::new()
        .format(|buf, record| {
            writeln!(
                buf,
                "[{} {}\x1b[0m] {}",
                Local::now().format("%Y-%m-%dT%H:%M:%S"),
                colorized_level(record.level()),
                record.args()
            )
        })
        // todo: 级别由 MIRAGEND_LOG 变量决定
        .filter(None, LevelFilter::Info)
        .init();
}

fn colorized_level(level: Level) -> &'static str {
    // Colors follow env_logger by default: https://github.com/rust-cli/env_logger/blob/73bb4188026f1ad5e6409c4148d37d572cc921bd/src/fmt/mod.rs#L168
    match level {
        Level::Error => "\x1b[31m\x1b[1mERROR",
        Level::Warn => "\x1b[33mWARN",
        Level::Info => "\x1b[32mINFO",
        Level::Debug => "\x1b[34mDEBUG",
        Level::Trace => "\x1b[36mTRACE",
    }
}

pub struct RoutedInfo<'a> {
    pub status_code: &'a StatusCode,
    pub path: &'a Uri,
    pub user_agent: &'a str,
    pub client_ip: String,
    pub referer: &'a str,
}

impl<'a> RoutedInfo<'a> {
    pub fn new(
        status_code: &'a StatusCode,
        path: &'a Uri,
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
        let referer = if let Some(referer) = req_headers.get(header::REFERER) {
            referer.to_str().unwrap_or("-")
        } else {
            "-"
        };

        RoutedInfo {
            status_code,
            path,
            user_agent,
            client_ip,
            referer,
        }
    }

    pub fn print_log(&self) {
        info!(
            "{} \"{}\" [Sent-to {}] [Client {}] \"{}\" \"{}\"",
            self.status_code,
            self.path,
            vars::upstream_base_url(),
            self.client_ip,
            self.user_agent,
            self.referer
        );
    }
}
