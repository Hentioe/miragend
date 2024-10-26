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
    pub body: String,
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
    let is_html = match resp.headers().get("content-type") {
        None => true,
        Some(header) => match header.to_str() {
            Ok(value) => value.starts_with("text/html"),
            Err(e) => {
                error!("illegal content-type: {}", e);

                return Loaded::Special(StatusCode::BAD_GATEWAY);
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

                return Loaded::Special(StatusCode::BAD_GATEWAY);
            }
        };
        Loaded::Forward(Response {
            status,
            headers,
            body,
        })
    } else {
        error!("response content-type is not supported");

        Loaded::Special(StatusCode::BAD_GATEWAY)
    }
}
