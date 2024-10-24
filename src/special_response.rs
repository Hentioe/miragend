use crate::vars::{self, CONTENT_TYPE_VALUE_TEXT_HTML};
use axum::{
    body::Body,
    response::{IntoResponse, Response},
};
use http::{header, StatusCode};
use log::error;

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Style {
    Nginx,
    None,
}

pub fn build_resp(status_code: StatusCode) -> Result<Response<Body>, http::Error> {
    let builder = Response::builder().status(status_code);
    let special_page_style = vars::special_page_style();
    let builder = if special_page_style == Style::Nginx {
        builder.header(header::CONTENT_TYPE, CONTENT_TYPE_VALUE_TEXT_HTML)
    } else {
        // No need to set the Content-Type header for the default style
        builder
    };

    builder.body(build_body(status_code, vars::special_page_style()))
}

pub fn build_resp_with_fallback(status_code: StatusCode) -> Response {
    match build_resp(status_code) {
        Ok(resp) => resp,
        Err(e) => {
            error!("{}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                StatusCode::INTERNAL_SERVER_ERROR.to_string(),
            )
                .into_response()
        }
    }
}

fn build_body(status_code: StatusCode, style: Style) -> Body {
    match style {
        Style::Nginx => build_nginx_page(status_code),
        Style::None => build_page(status_code),
    }
}

fn build_page(status_code: StatusCode) -> Body {
    Body::from(status_code.to_string())
}

// Reference: https://github.com/nginx/nginx/blob/master/src/http/ngx_http_special_response.c
fn build_nginx_page(status_code: StatusCode) -> Body {
    let content = match status_code {
        StatusCode::GATEWAY_TIMEOUT => "504 Gateway Time-out".to_owned(),
        StatusCode::INTERNAL_SERVER_ERROR => "500 Internal Server Error".to_owned(),
        StatusCode::BAD_GATEWAY => "502 Bad Gateway".to_owned(),
        _ => status_code.as_u16().to_string(),
    };
    let html = format!(
        "\
<html>
<head><title>{}</title></head>
<body>
<center><h1>{}</h1></center>
<hr><center>nginx</center>
</body>
</html>
<!-- a padding to disable MSIE and Chrome friendly error page -->
<!-- a padding to disable MSIE and Chrome friendly error page -->
<!-- a padding to disable MSIE and Chrome friendly error page -->
<!-- a padding to disable MSIE and Chrome friendly error page -->
<!-- a padding to disable MSIE and Chrome friendly error page -->
<!-- a padding to disable MSIE and Chrome friendly error page -->
",
        content, content
    );

    Body::from(html)
}
