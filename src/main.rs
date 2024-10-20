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

mod obfuscater;
mod vars;

// 后备补丁内容
const FALLBACK_PATCH_MARKDOWN: &str = include_str!("../patch-content.md");
// 忽略混淆文本的标签
const IGNORE_OBFUSCATE_TAGS: [&str; 5] = ["script", "noscript", "style", "template", "iframe"];
// 策略
enum Strategy {
    // 补丁
    Patch(PatchConfig),
    // 混淆
    Obfuscation,
}

struct PatchConfig {
    target: String,
    content: String,
    remove_nodes: Vec<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::init();

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

async fn handler(request: Request<Body>) -> Html<String> {
    let url = &format!("{}{}", vars::upstream_base_url(), request.uri());
    let strategy = match vars::strategy() {
        "patch" => {
            let patch_markdown = load_patch_content();
            let patch_html =
                comrak::markdown_to_html(&patch_markdown, &comrak::ComrakOptions::default());
            let config = PatchConfig {
                target: vars::patch_target().to_owned(),
                content: patch_html,
                remove_nodes: vars::remove_nodes().clone(),
            };
            Strategy::Patch(config)
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
        Strategy::Patch(config) => {
            let fragment_dom = parse_fragment(
                RcDom::default(),
                Default::default(),
                QualName::new(None, ns!(html), local_name!("body")),
                vec![],
            )
            .one(config.content);

            replace_children(
                dom.document.clone(),
                &config.target,
                find_elements(&fragment_dom.document),
            );
            for node in config.remove_nodes {
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
            Element { ref name, .. } => {
                if IGNORE_OBFUSCATE_TAGS.contains(&name.local.as_ref()) {
                    continue;
                } else {
                    obfuscate_text(child.clone());
                }
            }
            markup5ever_rcdom::NodeData::Text { ref contents } => {
                contents.replace_with(obfuscater::obfuscate_text);
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
    if !vars::patch_content_file().is_empty() {
        std::fs::read_to_string(Path::new(vars::patch_content_file()))
            .unwrap_or_else(|_| FALLBACK_PATCH_MARKDOWN.to_string())
    } else {
        FALLBACK_PATCH_MARKDOWN.to_string()
    }
}
