use anyhow::{anyhow, Context};
use axum::{body::Body, http::Request, response::Html, routing::get, Router};
use html5ever::tree_builder::TreeSink;
use log::info;
use scraper::{Html as ScraperHtml, Selector};

const BIND: &str = "0.0.0.0:8080";
// const FALLBACK_PAGE: &str =
//     "https://blog.hentioe.dev/posts/jujue-shiyong-moonshot-piaoqie-chanpin-kimi.html";

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
    let markdown_selector = Selector::parse(".markdown-body")
        .map_err(|e| anyhow!("failed to parse markdown selector: {}", e))?;
    // let fallback_html = {
    //     let body = reqwest::get(FALLBACK_PAGE)
    //         .await
    //         .context(format!("failed to fetch url: {}", FALLBACK_PAGE))?
    //         .text()
    //         .await
    //         .context("failed to read response body")?;

    //     let fallback = ScraperHtml::parse_document(&body);
    //     fallback
    //         .select(&markdown_selector)
    //         .next()
    //         .ok_or(anyhow!("markdown node not found"))?
    //         .inner_html()
    // };

    let body = reqwest::get(url)
        .await
        .context(format!("failed to fetch url: {}", url))?
        .text()
        .await
        .context("failed to read response body")?;

    let mut document = ScraperHtml::parse_document(&body);
    let content = document
        .select(&markdown_selector)
        .next()
        .ok_or(anyhow!("markdown node not found"))?;

    document.remove_from_parent(&content.id());

    Ok(document.html())
}
