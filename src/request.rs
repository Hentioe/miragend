use crate::vars;
use http::HeaderMap;
use reqwest::Response;
use std::time::Duration;

pub enum RequestError {
    Timeout,
    Reqwest(reqwest::Error),
}

pub async fn get(url: &str, headers: HeaderMap) -> Result<Response, RequestError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(vars::connect_timeout_secs()))
        .default_headers(headers)
        .build()
        .map_err(RequestError::Reqwest)?;

    match client.get(url).send().await {
        Ok(resp) => Ok(resp),
        Err(e) => Err(map_error(e)),
    }
}

fn map_error(e: reqwest::Error) -> RequestError {
    if e.is_timeout() {
        return RequestError::Timeout;
    }

    RequestError::Reqwest(e)
}
