use std::sync::LazyLock;

static BIND: LazyLock<String> =
    LazyLock::new(|| std::env::var("FAKE_BACKEND_BIND").unwrap_or("0.0.0.0:8080".to_owned()));
static UPSTREAM_BASE_URL: LazyLock<String> = LazyLock::new(|| {
    std::env::var("FAKE_BACKEND_UPSTREAM_BASE_URL")
        .expect("missing `FAKE_BACKEND_UPSTREAM_BASE_URL` env var")
});
static STRATEGY: LazyLock<String> =
    LazyLock::new(|| std::env::var("FAKE_BACKEND_STRATEGY").unwrap_or("obfuscation".to_owned()));
static PATCH_TARGET: LazyLock<String> =
    LazyLock::new(|| std::env::var("FAKE_BACKEND_PATCH_TARGET").unwrap_or_default());
static PATCH_CONTENT_FILE: LazyLock<String> =
    LazyLock::new(|| std::env::var("FAKE_BACKEND_PATCH_CONTENT_FILE").unwrap_or_default());
static REMOVE_NODES: LazyLock<Vec<String>> = LazyLock::new(|| {
    std::env::var("FAKE_BACKEND_REMOVE_NODES")
        .unwrap_or_default()
        .split(',')
        .map(|s| s.to_owned())
        .collect()
});

const FALLBACK_OBFUSCATED_MESTA_TAGS: [&str; 4] =
    ["description", "keywords", "og:title", "og:description"];

static OBFUSCATED_MESTA_TAGS: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    if let Ok(tags_text) = std::env::var("FAKE_BACKEND_OBFUSCATED_META_TAGS") {
        tags_text
            .split(',')
            .map(|s| Box::leak(s.to_owned().into_boxed_str()) as &'static str)
            .collect()
    } else {
        FALLBACK_OBFUSCATED_MESTA_TAGS.to_vec()
    }
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

pub fn remove_nodes() -> &'static Vec<String> {
    &REMOVE_NODES
}

pub fn obfuscated_meta_tags() -> &'static Vec<&'static str> {
    &OBFUSCATED_MESTA_TAGS
}
