use crate::request;
use http::{HeaderMap, StatusCode};
use log::error;

pub enum Loaded {
    Special(StatusCode),
    Forward(Response),
}

pub struct Response {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub content_type: ContentType,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, strum::Display)]
pub enum ContentType {
    Html,
    Json,
}

pub async fn load(url: &str, headers: HeaderMap) -> Loaded {
    let resp = match request::get(url, headers).await {
        Ok(resp) => resp,

        Err(request::RequestError::Timeout) => {
            return Loaded::Special(StatusCode::GATEWAY_TIMEOUT);
        }

        Err(request::RequestError::Reqwest(e)) => {
            error!("{}", e);
            return Loaded::Special(StatusCode::BAD_GATEWAY);
        }
    };

    // 读取 content-type，如果为空或 `text/html`，则返回 body
    let content_type = match resp.headers().get("content-type") {
        None => ContentType::Html,
        Some(header) => match header.to_str() {
            Ok(value) => {
                if value.starts_with("text/html") {
                    ContentType::Html
                } else if value.starts_with("application/json") {
                    ContentType::Json
                } else {
                    error!("unsupported content-type: {}", value);

                    return Loaded::Special(StatusCode::BAD_GATEWAY);
                }
            }
            Err(e) => {
                error!("illegal content-type: {}", e);

                return Loaded::Special(StatusCode::BAD_GATEWAY);
            }
        },
    };

    let status = resp.status();
    let headers = resp.headers().clone();
    let body = match resp.text().await {
        Ok(body) => body,
        Err(e) => {
            // 读取响应体失败
            error!("failed to read response body: {}", e);

            return Loaded::Special(StatusCode::BAD_GATEWAY);
        }
    };
    Loaded::Forward(Response {
        status,
        headers,
        content_type,
        body,
    })
}
