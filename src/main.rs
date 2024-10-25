use anyhow::{anyhow, Context};
use axum::body::Body;
use axum::response::Response;
use axum::{http::Request, routing::get, Router};
use clap::Parser;
use headers::AppendHeaders;
use html5ever::tendril::TendrilSink;
use html5ever::{parse_document, parse_fragment, serialize, LocalName, QualName};
use http::{HeaderMap, StatusCode, Uri};
use log::{error, info};
use markup5ever::{local_name, namespace_url, ns};
use markup5ever_rcdom::{Handle, Node, NodeData::Element, RcDom, SerializableHandle};
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

mod cli;
mod headers;
mod obfuscation;
mod request;
mod special_response;
mod vars;

// 后备补丁内容
const FALLBACK_PATCH_MARKDOWN: &str = include_str!("../patch-content.md");
const FALLBACK_PATCH_HTML: &str = include_str!("../patch-content.html");
// 忽略混淆文本的标签
const IGNORE_OBFUSCATION_TAGS: [&str; 5] = ["script", "noscript", "style", "template", "iframe"];
// 策略
enum Strategy<'a> {
    // 补丁
    Patch(PatchConfig<'a>),
    // 混淆
    Obfuscation,
}

struct PatchConfig<'a> {
    target: String,
    content: String,
    remove_nodes: &'a Vec<&'a str>,
    remove_meta_tags: &'a Vec<&'a str>,
}

enum Fetched {
    Special(StatusCode),
    Forward(RespForwarding),
}

struct RespForwarding {
    status: StatusCode,
    headers: HeaderMap,
    body: String,
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
        .await
        .context("failed to run server")?;

    Ok(())
}

fn validate_config() -> anyhow::Result<()> {
    if vars::upstream_base_url().is_empty() {
        return Err(anyhow!("missing `FAKE_BACKEND_UPSTREAM_BASE_URL` env var"));
    }

    Ok(())
}

async fn handler(request: Request<Body>) -> Response<Body> {
    let path = request.uri();
    let url = &format!("{}{}", vars::upstream_base_url(), path);
    let strategy = match vars::strategy() {
        "patch" => {
            let patch_html = load_patch_html(vars::patch_content_file());
            let config = PatchConfig {
                target: vars::patch_target().to_owned(),
                content: patch_html,
                remove_nodes: vars::patch_remove_nodes(),
                remove_meta_tags: vars::patch_remove_meta_tags(),
            };
            Strategy::Patch(config)
        }
        "obfuscation" | "obfus" => Strategy::Obfuscation,
        s => {
            // 无效的策略，回到后备策略
            error!("invalid strategy: `{}`, fallback strategy", s);
            Strategy::Obfuscation
        }
    };

    match fetch(url, headers::build_from_request(request.headers())).await {
        Fetched::Forward(forwarding) => match patch_page(&forwarding.body, &strategy).await {
            Ok(html) => {
                route_log(forwarding.status, path, url);

                match Response::builder()
                    .status(forwarding.status)
                    .append_headers(&forwarding.headers)
                    .body(Body::new(html))
                    .context("failed to create response")
                {
                    Ok(resp) => resp,
                    Err(e) => {
                        error!("{}", e);
                        special_response::build_resp_with_fallback(
                            StatusCode::INTERNAL_SERVER_ERROR,
                        )
                    }
                }
            }
            Err(e) => {
                route_log(StatusCode::INTERNAL_SERVER_ERROR, path, url);
                error!("{}", e);

                special_response::build_resp_with_fallback(StatusCode::INTERNAL_SERVER_ERROR)
            }
        },

        Fetched::Special(status_code) => {
            route_log(status_code, path, url);

            special_response::build_resp_with_fallback(status_code)
        }
    }
}

fn route_log(status_code: StatusCode, path: &Uri, url: &str) {
    info!("{} \"{}\" => \"{}\"", status_code, path, url);
}

async fn fetch(url: &str, headers: HeaderMap) -> Fetched {
    let resp = match request::get(url, headers).await {
        Ok(resp) => resp,

        Err(request::RequestError::Timeout) => {
            return Fetched::Special(StatusCode::GATEWAY_TIMEOUT);
        }

        Err(request::RequestError::Reqwest(e)) => {
            error!("{}", e);
            return Fetched::Special(StatusCode::BAD_GATEWAY);
        }
    };

    // 读取 content-type，如果为空或 `text/html`，则返回 body
    let is_html = match resp.headers().get("content-type") {
        None => true,
        Some(header) => match header.to_str() {
            Ok(value) => value.starts_with("text/html"),
            Err(e) => {
                error!("illegal content-type: {}", e);

                return Fetched::Special(StatusCode::BAD_GATEWAY);
            }
        },
    };

    if is_html {
        let status = resp.status();
        let headers = resp.headers().clone();
        let body = match resp.text().await {
            Ok(body) => body,
            Err(e) => {
                // 读取响应体失败
                error!("failed to read response body: {}", e);

                return Fetched::Special(StatusCode::BAD_GATEWAY);
            }
        };
        Fetched::Forward(RespForwarding {
            status,
            headers,
            body,
        })
    } else {
        error!("response content-type is not text/html");

        Fetched::Special(StatusCode::BAD_GATEWAY)
    }
}

async fn patch_page<'a>(html: &str, strategy: &'a Strategy<'_>) -> anyhow::Result<String> {
    let dom = parse_document(RcDom::default(), Default::default())
        .from_utf8()
        .read_from(&mut html.as_bytes())
        .context("failed to parse document")?;

    let _extending_lifecycle = match strategy {
        Strategy::Patch(config) => {
            let fragment_dom = parse_fragment(
                RcDom::default(),
                Default::default(),
                QualName::new(None, ns!(html), local_name!("body")),
                vec![],
            )
            .one(config.content.clone());

            replace_children(
                dom.document.clone(),
                &config.target,
                find_elements(&fragment_dom.document),
            );
            for node in config.remove_nodes {
                remove_children(dom.document.clone(), node);
            }
            remove_meta_tags(dom.document.clone(), config.remove_meta_tags);

            Some(fragment_dom)
        }
        Strategy::Obfuscation => {
            obfuscate_content(dom.document.clone());

            None
        }
    };

    let mut buf = Vec::new();
    let document: SerializableHandle = dom.document.clone().into();
    serialize(&mut buf, &document, Default::default()).context("failed to serialize document")?;
    let new_html = String::from_utf8(buf).context("failed to convert utf8")?;

    Ok(new_html)
}

fn replace_children(handle: Handle, target_id: &str, new_children: Vec<Rc<Node>>) {
    let node = handle;
    let children = node.children.borrow();
    for child in children.iter() {
        match &child.data {
            Element { ref attrs, .. } => {
                // 查找 id 属性
                let id = attrs.borrow().iter().find_map(|attr| {
                    if attr.name.local == local_name!("id") {
                        Some(attr.value.clone())
                    } else {
                        None
                    }
                });

                // 匹配目标 id
                if id.as_deref() == Some(target_id) {
                    child.children.replace(new_children);
                    return;
                } else {
                    replace_children(child.clone(), target_id, new_children.clone());
                }
            }
            _ => replace_children(child.clone(), target_id, new_children.clone()),
        }
    }
}
fn remove_children(handle: Handle, target_id: &str) {
    replace_children(handle, target_id, vec![])
}

fn obfuscate_content(handle: Handle) {
    let node = handle;
    let children = node.children.borrow();
    for child in children.iter() {
        match child.data {
            Element {
                ref name,
                ref attrs,
                ..
            } => {
                let find_attr = |name: LocalName| {
                    attrs
                        .borrow()
                        .iter()
                        .find(|attr| attr.name.local == name)
                        .map(|attr| attr.value.clone())
                };

                if let Some(id) = find_attr(local_name!("id")) {
                    if vars::obfuscation_ignore_nodes().contains(&id.as_ref()) {
                        continue;
                    }
                }

                let tag_name = name.local.as_ref();
                if IGNORE_OBFUSCATION_TAGS.contains(&tag_name) {
                    continue;
                } else if tag_name == "meta" {
                    // 混淆元标签的 content 属性：
                    // - 如果元标签的 name 是 OBFUSCATION_META_TAGS 之一，则混淆 content 属性
                    // - 如果元标签的 property 是 OBFUSCATION_META_TAGS 之一，则混淆 content 属性
                    let update_content = |name_or_property: &str| {
                        if vars::obfuscation_meta_tags().contains(&name_or_property) {
                            if let Some(mut content) = find_attr(local_name!("content")) {
                                attrs.borrow_mut().iter_mut().for_each(|attr| {
                                    if attr.name.local == local_name!("content") {
                                        attr.value = obfuscation::obfuscate_text(&mut content);
                                    }
                                });
                            }
                        }
                    };
                    if let Some(meta_name) = find_attr(local_name!("name")) {
                        update_content(&meta_name);
                    }

                    if let Some(meta_name) = find_attr(local_name!("property")) {
                        update_content(&meta_name);
                    }

                    continue;
                } else {
                    obfuscate_content(child.clone());
                }
            }
            markup5ever_rcdom::NodeData::Text { ref contents } => {
                contents.replace_with(obfuscation::obfuscate_text);
            }
            _ => obfuscate_content(child.clone()),
        }
    }
}

fn remove_meta_tags(handle: Handle, tags: &Vec<&str>) {
    let node = handle;
    let children = node.children.borrow();
    for child in children.iter() {
        match child.data {
            Element { ref name, .. } => {
                if name.local == local_name!("head") {
                    child.children.replace_with(|children| {
                        children.retain(|head_child| match head_child.data {
                            Element {
                                ref name,
                                ref attrs,
                                ..
                            } => {
                                if name.local == local_name!("meta") {
                                    let find_attr = |name: LocalName| {
                                        attrs
                                            .borrow()
                                            .iter()
                                            .find(|attr| attr.name.local == name)
                                            .map(|attr| attr.value.clone())
                                    };
                                    let mut is_retain = true;
                                    if let Some(meta_name) = find_attr(local_name!("name")) {
                                        if tags.contains(&meta_name.as_ref()) {
                                            is_retain = false;
                                        }
                                    }

                                    if let Some(meta_property) = find_attr(local_name!("property"))
                                    {
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
                    continue;
                } else {
                    remove_meta_tags(child.clone(), tags);
                }
            }
            _ => remove_meta_tags(child.clone(), tags),
        }
    }
}

fn find_elements(handle: &Handle) -> Vec<Rc<Node>> {
    let node: &Rc<Node> = handle;
    let children = node.children.borrow();
    if let Some(child) = children.iter().next() {
        match &child.data {
            Element { ref name, .. } => {
                if name.local.as_ref() == "html" {
                    // 将 child.children 转换为 Vec<Rc<Node>>
                    child.children.borrow().iter().cloned().collect()
                } else {
                    find_elements(child)
                }
            }
            _ => find_elements(child),
        }
    } else {
        // 这应该是一个 bug，请将您的补丁内容返回到 GitHub issue 中。
        // 解析补丁内容 HTML 时没有找到有效元素
        error!("no valid elements found when parsing patch content HTML");

        vec![Node::new(markup5ever_rcdom::NodeData::Text {
            contents: RefCell::new("".into()),
        })]
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
