//! Server assets.

use super::*;

pub(super) async fn static_file(State(app): State<App>, request: Request) -> Response {
    if request.uri().path().starts_with("/api/") {
        return error(StatusCode::NOT_FOUND, "unknown route").into_response();
    }
    if !matches!(*request.method(), Method::GET | Method::HEAD) {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    let path = request.uri().path();
    // `/chat/<id>` (ADR 0088) and `/setup` are page routes; the page reads its URL.
    let relative = if path == "/" || path == "/setup" || path.starts_with("/chat/") {
        "index.html"
    } else {
        path.trim_start_matches('/')
    };
    if relative.contains('%')
        || Path::new(relative)
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return StatusCode::NOT_FOUND.into_response();
    }
    let bytes: std::borrow::Cow<'static, [u8]> = match &app.assets {
        Some(assets) => match disk_file(assets, relative).await {
            Some(bytes) => bytes.into(),
            None => return StatusCode::NOT_FOUND.into_response(),
        },
        None => match WEB_ASSETS.iter().find(|(name, _)| *name == relative) {
            Some((_, bytes)) => (*bytes).into(),
            None => return StatusCode::NOT_FOUND.into_response(),
        },
    };
    let content_type = match Path::new(relative)
        .extension()
        .and_then(|extension| extension.to_str())
    {
        Some("html") => "text/html; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("webmanifest") => "application/manifest+json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("ico") => "image/x-icon",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    };
    let length = bytes.len();
    let mut response = Response::new(if request.method() == Method::HEAD {
        Body::empty()
    } else {
        Body::from(bytes)
    });
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    response.headers_mut().insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&length.to_string()).unwrap(),
    );
    response
}

/// A file under `--assets DIR`, refused when a link leads out of DIR.
pub(super) async fn disk_file(assets: &Path, relative: &str) -> Option<Vec<u8>> {
    let path = tokio::fs::canonicalize(assets.join(relative)).await.ok()?;
    if !path.starts_with(assets) || !tokio::fs::metadata(&path).await.ok()?.is_file() {
        return None;
    }
    tokio::fs::read(path).await.ok()
}
