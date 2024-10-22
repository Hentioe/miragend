use anyhow::{anyhow, Context};
use axum::body::Body;
use axum::response::{IntoResponse, Response};
use axum::{
    http::{header, Request},
    routing::get,
    Router,
};
use html5ever::tendril::TendrilSink;
use html5ever::{parse_document, parse_fragment, serialize, LocalName, QualName};
use log::{error, info};
use markup5ever::{local_name, namespace_url, ns};
use markup5ever_rcdom::{Handle, Node, NodeData::Element, RcDom, SerializableHandle};
use reqwest::StatusCode;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

mod obfuscation;
mod vars;

// 后备补丁内容
const FALLBACK_PATCH_MARKDOWN: &str = include_str!("../patch-content.md");
// 忽略混淆文本的标签
const IGNORE_OBFUSCATION_TAGS: [&str; 5] = ["script", "noscript", "style", "template", "iframe"];
// 502 错误内容
const BAD_GATEWAY_CONTENT: &str = "bad gateway";
// 500 错误内容
const INTERNAL_SERVER_ERROR_CONTENT: &str = "internal server error";
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

struct FetchResp {
    status: StatusCode,
    body: String,
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

async fn handler(request: Request<Body>) -> Response<Body> {
    let path = request.uri();
    let url = &format!("{}{}", vars::upstream_base_url(), path);
    let strategy = match vars::strategy() {
        "patch" => {
            let patch_markdown = load_patch_content();
            let patch_html =
                comrak::markdown_to_html(&patch_markdown, &comrak::ComrakOptions::default());
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

    match fetch_body(url).await {
        Ok(resp) => match patch_page(&resp.body, &strategy).await {
            Ok(html) => {
                info!("{} \"{}\" => \"{}\"", resp.status, request.uri(), url);

                match Response::builder()
                    .status(resp.status)
                    .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                    .body(Body::new(html))
                    .context("failed to create response")
                {
                    Ok(resp) => resp,
                    Err(e) => {
                        error!("{}", e);
                        build_500_resp()
                    }
                }
            }
            Err(e) => {
                info!(
                    "{} \"{}\" => \"{}\"",
                    StatusCode::INTERNAL_SERVER_ERROR,
                    request.uri(),
                    url
                );
                error!("{}", e);

                build_500_resp()
            }
        },

        Err(msg) => {
            info!(
                "{} \"{}\" => \"{}\"",
                StatusCode::BAD_GATEWAY,
                request.uri(),
                url
            );
            error!("{}", msg);

            (StatusCode::BAD_GATEWAY, BAD_GATEWAY_CONTENT).into_response()
        }
    }
}

async fn fetch_body(url: &str) -> anyhow::Result<FetchResp> {
    let resp = reqwest::get(url)
        .await
        .context(format!("failed to fetch url: {}", url))?;

    // 读取 content-type，如果为空或 `text/html`，则返回 body
    let is_html = match resp.headers().get("content-type") {
        None => true,
        Some(header) => {
            let value = header.to_str()?;
            value.starts_with("text/html")
        }
    };

    if is_html {
        Ok(FetchResp {
            status: resp.status(),
            body: resp.text().await.context("failed to read response body")?,
        })
    } else {
        Err(anyhow!("response content-type is not text/html"))
    }
}

fn build_500_resp() -> Response<Body> {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        INTERNAL_SERVER_ERROR_CONTENT,
    )
        .into_response()
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

fn load_patch_content() -> String {
    if !vars::patch_content_file().is_empty() {
        std::fs::read_to_string(Path::new(vars::patch_content_file()))
            .unwrap_or_else(|_| FALLBACK_PATCH_MARKDOWN.to_string())
    } else {
        FALLBACK_PATCH_MARKDOWN.to_string()
    }
}
