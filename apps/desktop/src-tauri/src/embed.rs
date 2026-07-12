use axum::{
    body::Body,
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "../../web/dist/"]
struct WebAssets;

pub(crate) async fn static_handler(uri: Uri) -> Response {
    let requested_path = uri.path().trim_start_matches('/');
    let path = if requested_path.is_empty() || WebAssets::get(requested_path).is_none() {
        "index.html"
    } else {
        requested_path
    };

    match WebAssets::get(path) {
        Some(file) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime.as_ref())
                .header(header::CACHE_CONTROL, cache_control(path))
                .body(Body::from(file.data.into_owned()))
                .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

fn cache_control(path: &str) -> &'static str {
    if path == "index.html" {
        "no-store"
    } else {
        "public, max-age=31536000, immutable"
    }
}
