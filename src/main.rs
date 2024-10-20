use anyhow::Context;
use axum::{body::Body, http::Request, response::Html, routing::get, Router};
use html5ever::tendril::TendrilSink;
use html5ever::{parse_document, parse_fragment, serialize, QualName};
use log::{error, info};
use markup5ever::{local_name, namespace_url, ns};
use markup5ever_rcdom::{Handle, Node, NodeData::Element, RcDom, SerializableHandle};
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::sync::LazyLock;

const BIND: &str = "0.0.0.0:8080";

static UPSTREAM_BASE_URL: LazyLock<String> = LazyLock::new(|| {
    std::env::var("FAKE_BACKEND_UPSTREAM_BASE_URL")
        .expect("missing `FAKE_BACKEND_UPSTREAM_BASE_URL` env var")
});
static STRATEGY: LazyLock<String> =
    LazyLock::new(|| std::env::var("FAKE_BACKEND_STRATEGY").unwrap_or("obfuscation".to_owned()));
static PATCH_CONTENT_FILE: LazyLock<String> =
    LazyLock::new(|| std::env::var("FAKE_BACKEND_PATCH_CONTENT_FILE").unwrap_or_default());
static REMOVE_NODES: LazyLock<Vec<String>> = LazyLock::new(|| {
    std::env::var("FAKE_BACKEND_REMOVE_NODES")
        .unwrap_or_default()
        .split(',')
        .map(|s| s.to_owned())
        .collect()
});

// 从文件中读取后备补丁内容
const FALLBACK_PATCH_MARKDOWN: &str = include_str!("../patch-content.md");

// 策略
enum Strategy {
    // 补丁
    Patch(String, Vec<String>),
    // 混淆
    Obfuscation,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

    let app = Router::new().route("/*path", get(handler));
    let listener = tokio::net::TcpListener::bind(BIND)
        .await
        .context("failed to bind to address")?;

    info!("listening on: http://{}", BIND);

    axum::serve(listener, app)
        .await
        .context("failed to run server")?;

    Ok(())
}

async fn handler(request: Request<Body>) -> Html<String> {
    let url = &format!("{}{}", *UPSTREAM_BASE_URL, request.uri());
    let strategy = match (STRATEGY).as_str() {
        "patch" => {
            let patch_markdown = load_patch_content();
            let patch_html =
                comrak::markdown_to_html(&patch_markdown, &comrak::ComrakOptions::default());
            Strategy::Patch(patch_html, REMOVE_NODES.clone())
        }
        "obfuscation" => Strategy::Obfuscation,
        s => {
            // 无效的策略，回到后备策略
            error!("invalid strategy: `{}`, fallback strategy", s);
            Strategy::Obfuscation
        }
    };

    match patch_page(url, strategy).await {
        Ok(html) => Html(html),
        Err(e) => Html(format!("Error: {}", e)),
    }
}

async fn patch_page(url: &str, strategy: Strategy) -> anyhow::Result<String> {
    let body = reqwest::get(url)
        .await
        .context(format!("failed to fetch url: {}", url))?
        .text()
        .await
        .context("failed to read response body")?;

    let dom = parse_document(RcDom::default(), Default::default())
        .from_utf8()
        .read_from(&mut body.as_bytes())
        .context("failed to parse document")?;

    let _extending_lifecycle = match strategy {
        Strategy::Patch(patch_content, remove_nodes) => {
            let fragment_dom = parse_fragment(
                RcDom::default(),
                Default::default(),
                QualName::new(None, ns!(html), local_name!("body")),
                vec![],
            )
            .one(patch_content.clone());

            replace_children(
                dom.document.clone(),
                "post-content",
                find_elements(&fragment_dom.document),
            );
            for node in remove_nodes {
                remove_children(dom.document.clone(), &node);
            }

            Some(fragment_dom)
        }
        Strategy::Obfuscation => {
            obfuscate_text(dom.document.clone());

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

fn obfuscate_text(handle: Handle) {
    let node = handle;
    let children = node.children.borrow();
    for child in children.iter() {
        match child.data {
            markup5ever_rcdom::NodeData::Text { ref contents } => {
                contents.replace_with(|text| {
                    text.chars()
                        .map(|_c| {
                            // 一个随机的生僻汉字
                            rand::random::<u32>() % (0x9fa5 - 0x4e00) + 0x4e00
                        })
                        .map(|c| std::char::from_u32(c).unwrap())
                        .collect()
                });
            }
            _ => obfuscate_text(child.clone()),
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

fn load_patch_content() -> String {
    if !PATCH_CONTENT_FILE.is_empty() {
        std::fs::read_to_string(Path::new(&*PATCH_CONTENT_FILE))
            .unwrap_or_else(|_| FALLBACK_PATCH_MARKDOWN.to_string())
    } else {
        FALLBACK_PATCH_MARKDOWN.to_string()
    }
}
