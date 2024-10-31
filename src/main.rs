use anyhow::Context;
use axum::body::Body;
use axum::{http::Request, routing::get, Router};
use clap::Parser;
use fetching::Loaded;
use headers::AppendHeaders;
use html5ever::LocalName;
use html_ops::{DOMBuilder, DOMOps, NodeOps};
use http::{header, HeaderMap, Response, StatusCode, Uri};
use log::{error, info, warn};
use markup5ever::local_name;
use markup5ever_rcdom::{Handle, Node, NodeData::Element};
use obfuscation::Obfuscator;
use std::path::Path;
use std::rc::Rc;
use std::str::Chars;
use tokio::signal;

mod cli;
mod fetching;
mod headers;
mod html_ops;
mod obfuscation;
mod request;
mod special_response;
mod vars;

// Fallback patch contents
const FALLBACK_PATCH_MARKDOWN: &str = include_str!("../patch-content.md");
const FALLBACK_PATCH_HTML: &str = include_str!("../patch-content.html");
// Ignore obfuscation for these tags
const IGNORE_OBFUSCATION_TAGS: [&str; 5] = ["script", "noscript", "style", "template", "iframe"];
// Strategy configuration
enum Strategy<'a> {
    // Patch
    Patch(PatchConfig<'a>),
    // Obfuscation
    Obfuscation,
}

struct PatchConfig<'a> {
    target: String,
    content: String,
    remove_nodes: &'a Vec<&'a str>,
    remove_meta_tags: &'a Vec<&'a str>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();
    if dotenvy::dotenv().is_ok() {
        info!("loaded .env file");
    }
    validate_config()?;
    let _args = cli::Args::parse();
    let app = Router::new().route("/*path", get(handler));
    let bind = vars::bind();
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .context("failed to bind to address")?;

    info!("listening on: http://{}", bind);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .context("failed to run server")?;

    Ok(())
}

fn validate_config() -> anyhow::Result<()> {
    vars::force_init();

    Ok(())
}

async fn handler(request: Request<Body>) -> Response<Body> {
    match vars::strategy() {
        "patch" => patch_handler(request).await,
        "obfuscation" | "obfus" => obfus_handler(request).await,
        s => {
            error!("invalid strategy: {}, fallback to obfuscation", s);

            obfus_handler(request).await
        }
    }
}

async fn obfus_handler(request: Request<Body>) -> Response<Body> {
    handle(request, Strategy::Obfuscation).await
}

async fn patch_handler(request: Request<Body>) -> Response<Body> {
    let patch_html = load_patch_html(vars::patch_content_file());
    let config = PatchConfig {
        target: vars::patch_target().to_owned(),
        content: patch_html,
        remove_nodes: vars::patch_remove_nodes(),
        remove_meta_tags: vars::patch_remove_meta_tags(),
    };

    handle(request, Strategy::Patch(config)).await
}

async fn handle(request: Request<Body>, strategy: Strategy<'_>) -> Response<Body> {
    use fetching::ContentType::*;
    use special_response::build_resp_with_fallback;

    let path = request.uri();
    let url = &format!("{}{}", vars::upstream_base_url(), path);
    let headers = headers::build_from_request(request.headers());
    let build_resp = |resp: &fetching::Response, body: String| {
        Response::builder()
            .status(resp.status)
            .append_headers(&resp.headers)
            .body(Body::new(body))
            .context("failed to create response")
    };

    match fetching::load(url, headers.clone()).await {
        Loaded::Forward(resp) if resp.content_type == Html => {
            match handle_page(&resp.body, &strategy).await {
                Ok(html) => match build_resp(&resp, html) {
                    Ok(resp) => {
                        route_log(&resp.status(), path, url, &headers);

                        resp
                    }
                    Err(e) => {
                        route_log(&StatusCode::INTERNAL_SERVER_ERROR, path, url, &headers);

                        error!("{}", e);
                        build_resp_with_fallback(StatusCode::INTERNAL_SERVER_ERROR)
                    }
                },
                Err(e) => {
                    route_log(&StatusCode::INTERNAL_SERVER_ERROR, path, url, &headers);
                    error!("{}", e);

                    build_resp_with_fallback(StatusCode::INTERNAL_SERVER_ERROR)
                }
            }
        }
        Loaded::Forward(resp) if resp.content_type == Json => {
            match handle_json(&resp.body, &strategy) {
                Ok(json) => match build_resp(&resp, json) {
                    Ok(resp) => {
                        route_log(&resp.status(), path, url, &headers);

                        resp
                    }
                    Err(e) => {
                        route_log(&StatusCode::INTERNAL_SERVER_ERROR, path, url, &headers);

                        error!("{}", e);
                        build_resp_with_fallback(StatusCode::INTERNAL_SERVER_ERROR)
                    }
                },
                Err(e) => {
                    route_log(&StatusCode::INTERNAL_SERVER_ERROR, path, url, &headers);

                    error!("{}", e);
                    build_resp_with_fallback(StatusCode::INTERNAL_SERVER_ERROR)
                }
            }
        }
        Loaded::Forward(resp) => {
            error!("unhandled content-type: {}", resp.content_type);

            build_resp_with_fallback(StatusCode::INTERNAL_SERVER_ERROR)
        }
        Loaded::Special(status_code) => {
            route_log(&status_code, path, url, &headers);

            build_resp_with_fallback(status_code)
        }
    }
}

fn route_log(status_code: &StatusCode, path: &Uri, url: &str, headers: &HeaderMap) {
    let user_agent = headers
        .get(header::USER_AGENT)
        .map(|v| v.to_str().unwrap_or_default())
        .unwrap_or_default();

    info!(
        "{} \"{}\" => \"{}\" \"{}\"",
        status_code, path, url, user_agent
    );
}

async fn handle_page<'a>(html: &str, strategy: &'a Strategy<'_>) -> anyhow::Result<String> {
    let dom = html.build_document().context("failed to parse document")?;

    let _extending_lifecycle = match strategy {
        Strategy::Patch(config) => {
            let fragment_dom = config.content.build_fragment();
            replace_children(
                dom.document.clone(),
                &config.target,
                html_ops::extract_contents(&fragment_dom.document),
            );
            for node in config.remove_nodes {
                remove_children(dom.document.clone(), node);
            }
            remove_doc_metas(dom.document.clone(), config.remove_meta_tags);

            Some(fragment_dom)
        }
        Strategy::Obfuscation => {
            obfuscate_doc_text(dom.document.clone(), vars::obfuscation_ignore_len());
            obfuscate_doc_metas(dom.document.clone(), vars::obfuscation_meta_tags());

            None
        }
    };

    let inject_script = vars::inject_online_script();
    if !inject_script.is_empty() {
        inject_online_script(dom.document.clone(), inject_script);
    }

    html_ops::serialize_to_html(dom).context("failed to serialize document")
}

fn handle_json(json: &str, strategy: &Strategy<'_>) -> anyhow::Result<String> {
    let mut map: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(json).context("failed to parse JSON")?;
    match strategy {
        Strategy::Patch(_) => Ok(json.to_owned()),
        Strategy::Obfuscation => {
            map.obfuscate();

            serde_json::to_string(&map).context("failed to serialize JSON")
        }
    }
}

fn replace_children(handle: Handle, node_id: &str, new_children: Vec<Rc<Node>>) {
    if let Some(node) = handle.get_element_by_id(node_id) {
        node.children.replace(new_children);
    } else {
        warn!("node with id `{}` not found", node_id);
    }
}

fn remove_children(handle: Handle, node_id: &str) {
    replace_children(handle, node_id, vec![])
}

fn obfuscate_doc_text(handle: Handle, mut ignore_remaining: usize) {
    let mut text_nodes: Vec<(Rc<Node>, bool)> = vec![];
    collect_obfuscation_nodes(&handle, &mut text_nodes, false, false);
    // let children = handle.children.borrow();
    for (child, after_content) in text_nodes {
        if let markup5ever_rcdom::NodeData::Text { ref contents } = child.data {
            contents.replace_with(|text| {
                if !after_content || ignore_remaining == 0 {
                    text.obfuscated()
                } else {
                    let (content, remaining) =
                        obfuscated_with_remaining(text.chars(), ignore_remaining);
                    ignore_remaining = remaining;

                    content.into()
                }
            });
        }
    }
}

fn obfuscated_with_remaining(chars: Chars<'_>, mut ignore_remaining: usize) -> (String, usize) {
    let mut parts = vec![];
    for c in chars {
        // 如果不是空白字符
        let c = if ignore_remaining > 0 && !c.is_whitespace() {
            ignore_remaining -= 1;

            c
        } else {
            c.obfuscated()
        };

        parts.push(c);
    }

    (parts.into_iter().collect(), ignore_remaining)
}

fn collect_obfuscation_nodes(
    handle: &Handle,
    text_nodes: &mut Vec<(Handle, bool)>,
    mut title_found: bool,
    mut after_content: bool,
) {
    let children = handle.children.borrow();
    for child in children.iter() {
        match child.data {
            markup5ever_rcdom::NodeData::Text { .. } => {
                let parent_is_title = || match handle.data {
                    Element { ref name, .. } => name.local == local_name!("title"),
                    _ => false,
                };
                if !title_found && vars::obfuscation_ignore_title() && parent_is_title() {
                    // No obfuscation for title
                    title_found = true;
                } else {
                    text_nodes.push((child.clone(), after_content));
                }
            }
            markup5ever_rcdom::NodeData::Element { ref name, .. } => {
                if let Some(id) = child.get_attribute(&local_name!("id")) {
                    // Check if node is in ignore list (from config)
                    if vars::obfuscation_ignore_nodes().contains(&id.as_ref()) {
                        // Skip obfuscation
                        continue;
                    }

                    // TODO: 提取此处的 obfuscation_ignore_after_node 作为参数
                    if id.as_ref() == vars::obfuscation_ignore_after_node() {
                        after_content = true;
                    }
                }

                let tag_name = name.local.as_ref();
                // Check if tag is in ignore list
                if IGNORE_OBFUSCATION_TAGS.contains(&tag_name) {
                    // Skip obfuscation
                    continue;
                } else {
                    collect_obfuscation_nodes(child, text_nodes, title_found, after_content)
                }
            }
            _ => {}
        }
    }
}

fn obfuscate_doc_metas(handle: Handle, include_tags: &[&str]) {
    for mut meta_tag in handle.find_meta_tags() {
        let content_locale_name = local_name!("content");
        let mut update_content = |attr_name: &LocalName| {
            if let Some(meta_name) = meta_tag.get_attribute(attr_name) {
                if include_tags.contains(&meta_name.as_ref()) {
                    if let Some(content) = meta_tag.get_attribute(&content_locale_name).as_mut() {
                        meta_tag.set_attribute(&content_locale_name, content.obfuscated());
                    }
                }
            }
        };
        update_content(&local_name!("name"));
        update_content(&local_name!("property"));
    }
}

fn remove_doc_metas(handle: Handle, tags: &[&str]) {
    if let Some(head) = handle.get_head() {
        let name_local_name = local_name!("name");
        let property_local_name = local_name!("property");
        let meta_local_name = local_name!("meta");
        head.children.replace_with(|children| {
            children.retain(|child| match child.data {
                Element { ref name, .. } => {
                    if name.local == meta_local_name {
                        let mut is_retain = true;
                        if let Some(meta_name) = child.get_attribute(&name_local_name) {
                            if tags.contains(&meta_name.as_ref()) {
                                is_retain = false;
                            }
                        }

                        if let Some(meta_property) = child.get_attribute(&property_local_name) {
                            if tags.contains(&meta_property.as_ref()) {
                                is_retain = false;
                            }
                        }

                        is_retain
                    } else {
                        true
                    }
                }
                _ => true,
            });

            children.to_vec()
        });
    }
}

fn inject_online_script(handle: Handle, url: &str) {
    if let Some(head) = handle.get_head() {
        // 创建一个 script 节点
        let mut head_children = head.children.borrow_mut();
        head_children.push(html_ops::build_script(url.into()));
        head_children.push(html_ops::build_newline());
    }
}

fn load_patch_html(patch_content_file: &str) -> String {
    if patch_content_file.is_empty() {
        let markdown = FALLBACK_PATCH_MARKDOWN.to_string();

        markdown_to_html(&markdown)
    } else if patch_content_file.ends_with(".md") {
        let markdown = std::fs::read_to_string(Path::new(patch_content_file))
            .unwrap_or_else(|_| FALLBACK_PATCH_MARKDOWN.to_string());

        markdown_to_html(&markdown)
    } else if patch_content_file.ends_with(".html") {
        std::fs::read_to_string(Path::new(patch_content_file))
            .unwrap_or_else(|_| FALLBACK_PATCH_HTML.to_string())
    } else {
        let text = std::fs::read_to_string(Path::new(patch_content_file))
            .unwrap_or_else(|_| "Hello from Miragend!".to_owned());

        // Split text by newlines and wrap each line in <p> tags
        text.lines().fold(String::new(), |acc, line| {
            format!("{}\n<p>{}</p>", acc, line)
        })
    }
}

fn markdown_to_html(markdown: &str) -> String {
    comrak::markdown_to_html(markdown, &comrak::ComrakOptions::default())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
