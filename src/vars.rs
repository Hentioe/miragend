use crate::{obfuscation::ObfuscatorConfig, special_response};
use http::HeaderValue;
use log::warn;
use std::{fs, path::PathBuf, sync::LazyLock};

static BIND: LazyLock<String> =
    LazyLock::new(|| std::env::var("MIRAGEND_BIND").unwrap_or("0.0.0.0:8080".to_owned()));
static UPSTREAM_BASE_URL: LazyLock<String> = LazyLock::new(|| {
    std::env::var("MIRAGEND_UPSTREAM_BASE_URL").expect("missing `UPSTREAM_BASE_URL` env var")
});
static UPSTREAM_DOAMIN: LazyLock<HeaderValue> = LazyLock::new(|| {
    let url = reqwest::Url::parse(&UPSTREAM_BASE_URL).expect("invalid `UPSTREAM_BASE_URL` value");
    let domain = url
        .domain()
        .expect("missing domain in `UPSTREAM_BASE_URL` value")
        .to_owned();

    HeaderValue::from_str(&domain).expect("invalid header value in `UPSTREAM_BASE_URL` value")
});
static STRATEGY: LazyLock<String> =
    LazyLock::new(|| std::env::var("MIRAGEND_STRATEGY").unwrap_or("obfuscation".to_owned()));
static PATCH_TARGET: LazyLock<String> =
    LazyLock::new(|| std::env::var("MIRAGEND_PATCH_TARGET").unwrap_or_default());
static PATCH_CONTENT_FILE: LazyLock<String> =
    LazyLock::new(|| std::env::var("MIRAGEND_PATCH_CONTENT_FILE").unwrap_or_default());
static PATCH_REMOVE_NODES: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    let text = std::env::var("MIRAGEND_PATCH_REMOVE_NODES").unwrap_or_default();
    if !text.is_empty() {
        text.split(',')
            .map(|s| Box::leak(s.to_owned().into_boxed_str()) as &'static str)
            .collect()
    } else {
        Vec::new()
    }
});
static PATCH_REMOVE_META_TAGS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    std::env::var("MIRAGEND_PATCH_REMOVE_META_TAGS")
        .unwrap_or_default()
        .split(',')
        .map(|s| Box::leak(s.to_owned().into_boxed_str()) as &'static str)
        .collect()
});
const FALLBACK_OBFUSCATION_MESTA_TAGS: [&str; 4] =
    ["description", "keywords", "og:title", "og:description"];
static OBFUSCATION_MESTA_TAGS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    if let Ok(tags_text) = std::env::var("MIRAGEND_OBFUSCATION_META_TAGS") {
        tags_text
            .split(',')
            .map(|s| Box::leak(s.to_owned().into_boxed_str()) as &'static str)
            .collect()
    } else {
        FALLBACK_OBFUSCATION_MESTA_TAGS.to_vec()
    }
});
static OBFUSCATION_IGNORE_NDOES: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    std::env::var("MIRAGEND_OBFUSCATION_IGNORE_NODES")
        .unwrap_or_default()
        .split(',')
        .map(|s| Box::leak(s.to_owned().into_boxed_str()) as &'static str)
        .collect()
});
static OBFUSCATION_IGNORE_TITLE: LazyLock<bool> = LazyLock::new(|| {
    if let Ok(v) = std::env::var("MIRAGEND_OBFUSCATION_IGNORE_TITLE") {
        if ["true", "false"].contains(&v.as_str()) {
            v == "true"
        } else {
            warn!("invalid value for `MIRAGEND_OBFUSCATION_IGNORE_TITLE`, expected `true` or `false`, got `{}`", v);
            false
        }
    } else {
        false
    }
});
static OBFUSCATION_IGNORE_AFTER_NODE: LazyLock<String> =
    LazyLock::new(|| std::env::var("MIRAGEND_OBFUSCATION_IGNORE_AFTER_NODE").unwrap_or_default());
static OBFUSCATION_IGNORE_LEN: LazyLock<usize> = LazyLock::new(|| {
    std::env::var("MIRAGEND_OBFUSCATION_IGNORE_LEN")
        .unwrap_or("0".to_owned())
        .parse()
        .unwrap_or(0)
});
static OBFUSCATION_MAPPING_FILE: LazyLock<String> =
    LazyLock::new(|| std::env::var("MIRAGEND_OBFUSCATION_MAPPING_FILE").unwrap_or_default());
const DEFAULT_TIMEOUT_SECS: u64 = 60;
static CONNECT_TIMEOUT_SECS: LazyLock<u64> = LazyLock::new(|| {
    std::env::var("MIRAGEND_CONNECT_TIMEOUT_SECS")
        .unwrap_or(DEFAULT_TIMEOUT_SECS.to_string())
        .parse()
        .unwrap_or(DEFAULT_TIMEOUT_SECS)
});
static SPECIAL_PAGE_STYLE: LazyLock<special_response::Style> =
    LazyLock::new(|| {
        match std::env::var("MIRAGEND_SPECIAL_PAGE_STYLE")
            .unwrap_or_default()
            .as_str()
        {
            "nginx" => special_response::Style::Nginx,
            _ => special_response::Style::None,
        }
    });
static INJECT_ONLINE_SCRIPT: LazyLock<String> =
    LazyLock::new(|| std::env::var("MIRAGEND_INJECT_ONLINE_SCRIPT").unwrap_or_default());
static OBFUSCATOR_CONFIG: LazyLock<ObfuscatorConfig> = LazyLock::new(|| {
    let csv_content = if OBFUSCATION_MAPPING_FILE.is_empty()
        || !PathBuf::from(&*OBFUSCATION_MAPPING_FILE).exists()
    {
        include_str!("../obfuscation_mapping.csv")
    } else {
        &fs::read_to_string(&*OBFUSCATION_MAPPING_FILE)
            .expect("failed to read obfuscator mapping file")
    };
    ObfuscatorConfig::load_from_csv(csv_content)
});
pub const CONTENT_TYPE_VALUE_TEXT_HTML: &str = "text/html; charset=utf-8";

// Call on startup to avoid runtime initialization errors
pub fn force_init() {
    LazyLock::force(&UPSTREAM_BASE_URL);
    LazyLock::force(&UPSTREAM_DOAMIN);
    LazyLock::force(&OBFUSCATOR_CONFIG);
    LazyLock::force(&OBFUSCATION_IGNORE_TITLE);
}

pub fn bind() -> &'static str {
    &BIND
}

pub fn upstream_base_url() -> &'static str {
    &UPSTREAM_BASE_URL
}

pub fn upstream_domain() -> &'static HeaderValue {
    &UPSTREAM_DOAMIN
}

pub fn strategy() -> &'static str {
    &STRATEGY
}

pub fn patch_target() -> &'static str {
    &PATCH_TARGET
}

pub fn patch_content_file() -> &'static str {
    &PATCH_CONTENT_FILE
}

pub fn patch_remove_nodes() -> &'static Vec<&'static str> {
    &PATCH_REMOVE_NODES
}

pub fn patch_remove_meta_tags() -> &'static Vec<&'static str> {
    &PATCH_REMOVE_META_TAGS
}

pub fn obfuscation_meta_tags() -> &'static Vec<&'static str> {
    &OBFUSCATION_MESTA_TAGS
}

pub fn obfuscation_ignore_nodes() -> &'static Vec<&'static str> {
    &OBFUSCATION_IGNORE_NDOES
}

pub fn obfuscation_ignore_title() -> bool {
    *OBFUSCATION_IGNORE_TITLE
}

pub fn obfuscation_ignore_after_node() -> &'static str {
    &OBFUSCATION_IGNORE_AFTER_NODE
}

pub fn obfuscation_ignore_len() -> usize {
    *OBFUSCATION_IGNORE_LEN
}

pub fn obfuscator_config() -> &'static ObfuscatorConfig {
    &OBFUSCATOR_CONFIG
}

pub fn connect_timeout_secs() -> u64 {
    *CONNECT_TIMEOUT_SECS
}

pub fn special_page_style() -> special_response::Style {
    *SPECIAL_PAGE_STYLE
}

pub fn inject_online_script() -> &'static str {
    &INJECT_ONLINE_SCRIPT
}
