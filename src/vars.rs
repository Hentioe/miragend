use std::sync::LazyLock;

static BIND: LazyLock<String> =
    LazyLock::new(|| std::env::var("FAKE_BACKEND_BIND").unwrap_or("0.0.0.0:8080".to_owned()));
static UPSTREAM_BASE_URL: LazyLock<String> =
    LazyLock::new(|| std::env::var("FAKE_BACKEND_UPSTREAM_BASE_URL").unwrap_or_default());
static STRATEGY: LazyLock<String> =
    LazyLock::new(|| std::env::var("FAKE_BACKEND_STRATEGY").unwrap_or("obfuscation".to_owned()));
static PATCH_TARGET: LazyLock<String> =
    LazyLock::new(|| std::env::var("FAKE_BACKEND_PATCH_TARGET").unwrap_or_default());
static PATCH_CONTENT_FILE: LazyLock<String> =
    LazyLock::new(|| std::env::var("FAKE_BACKEND_PATCH_CONTENT_FILE").unwrap_or_default());
static PATCH_REMOVE_NODES: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    std::env::var("FAKE_BACKEND_PATCH_REMOVE_NODES")
        .unwrap_or_default()
        .split(',')
        .map(|s| Box::leak(s.to_owned().into_boxed_str()) as &'static str)
        .collect()
});
static PATCH_REMOVE_META_TAGS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    std::env::var("FAKE_BACKEND_PATCH_REMOVE_META_TAGS")
        .unwrap_or_default()
        .split(',')
        .map(|s| Box::leak(s.to_owned().into_boxed_str()) as &'static str)
        .collect()
});
const FALLBACK_OBFUSCATION_MESTA_TAGS: [&str; 4] =
    ["description", "keywords", "og:title", "og:description"];
static OBFUSCATION_MESTA_TAGS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    if let Ok(tags_text) = std::env::var("FAKE_BACKEND_OBFUSCATION_META_TAGS") {
        tags_text
            .split(',')
            .map(|s| Box::leak(s.to_owned().into_boxed_str()) as &'static str)
            .collect()
    } else {
        FALLBACK_OBFUSCATION_MESTA_TAGS.to_vec()
    }
});
static OBFUSCATION_IGNORE_NDOES: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    std::env::var("FAKE_BACKEND_OBFUSCATION_IGNORE_NODES")
        .unwrap_or_default()
        .split(',')
        .map(|s| Box::leak(s.to_owned().into_boxed_str()) as &'static str)
        .collect()
});
const DEFAULT_TIMEOUT_SECS: u64 = 60;
static CONNECT_TIMEOUT_SECS: LazyLock<u64> = LazyLock::new(|| {
    std::env::var("FAKE_BACKEND_CONNECT_TIMEOUT_SECS")
        .unwrap_or(DEFAULT_TIMEOUT_SECS.to_string())
        .parse()
        .unwrap_or(DEFAULT_TIMEOUT_SECS)
});

pub fn bind() -> &'static str {
    &BIND
}

pub fn upstream_base_url() -> &'static str {
    &UPSTREAM_BASE_URL
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

pub fn connect_timeout_secs() -> u64 {
    *CONNECT_TIMEOUT_SECS
}
