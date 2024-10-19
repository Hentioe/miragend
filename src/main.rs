use anyhow::Context;
use axum::{body::Body, http::Request, response::Html, routing::get, Router};
use html5ever::tendril::TendrilSink;
use html5ever::{parse_document, parse_fragment, serialize, QualName};
use log::info;
use markup5ever::{local_name, namespace_url, ns};
use markup5ever_rcdom::{Handle, Node, RcDom, SerializableHandle};
use std::rc::Rc;

const BIND: &str = "0.0.0.0:8080";

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
    let uri = request.uri().to_string();
    match patch_page(&format!("https://blog.hentioe.dev{}", uri)).await {
        Ok(html) => Html(html),
        Err(e) => Html(format!("Error: {}", e)),
    }
}

async fn patch_page(url: &str) -> anyhow::Result<String> {
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

    let fragment_dom = parse_fragment(
        RcDom::default(),
        Default::default(),
        QualName::new(None, ns!(html), local_name!("body")),
        vec![],
    )
    .one("Fuck you, Kimi");

    let child = find_first_element(&fragment_dom.document);

    replace_children(dom.document.clone(), "post-content", vec![child]);
    remove_children(dom.document.clone(), "TableOfContents");

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
            markup5ever_rcdom::NodeData::Element { ref attrs, .. } => {
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

fn find_first_element(handle: &Handle) -> Rc<Node> {
    let node = handle;
    let children = node.children.borrow();
    if let Some(child) = children.iter().next() {
        match &child.data {
            markup5ever_rcdom::NodeData::Element { ref name, .. } => {
                if name.local.as_ref() != "html" && name.local.as_ref() != "body" {
                    child.clone()
                } else {
                    find_first_element(child)
                }
            }
            markup5ever_rcdom::NodeData::Text { .. } => child.clone(),
            _ => find_first_element(child),
        }
    } else {
        panic!("No element found");
    }
}
