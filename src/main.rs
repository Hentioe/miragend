use anyhow::{anyhow, Context};
use axum::{body::Body, http::Request, response::Html, routing::get, Router};
use html5ever::tree_builder::TreeSink;
use log::info;
use scraper::{Html as ScraperHtml, Selector};

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

    let mut document = ScraperHtml::parse_document(&body);
    let markdown_selector = Selector::parse(".markdown-body")
        .map_err(|e| anyhow!("failed to parse markdown selector: {}", e))?;
    let markdown_body = document
        .select(&markdown_selector)
        .next()
        .ok_or(anyhow!("markdown node not found"))?;

    let toc_content_selector = Selector::parse("#TableOfContents")
        .map_err(|e| anyhow!("failed to parse toc selector: {}", e))?;
    let toc_content = document
        .select(&toc_content_selector)
        .next()
        .ok_or(anyhow!("toc contents not found"))?;
    let markdown_body_handle = markdown_body.id();
    let toc_content_handle = toc_content.id();
    document.remove_from_parent(&markdown_body_handle);
    document.remove_from_parent(&toc_content_handle);

    Ok(document.html())
}
