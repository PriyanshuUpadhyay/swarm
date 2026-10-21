//! Server guards.

use super::*;

pub(super) async fn guard(State(app): State<App>, request: Request, next: Next) -> Response {
    if !request.uri().path().starts_with("/api/") {
        return next.run(request).await;
    }
    // A WebSocket cannot send the bearer header; its single-use ticket stands in for it, and the
    // Host and a required Origin still apply (terminal.rs).
    let socket = request.uri().path() == SOCKET_PATH;
    let authorization = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok());
    let expected = format!("Bearer {}", app.token);
    if !socket && authorization != Some(expected.as_str()) {
        let mut response = error(StatusCode::UNAUTHORIZED, "bearer token required").into_response();
        let challenge = if authorization.is_none() {
            "Bearer realm=\"swarm\""
        } else {
            "Bearer realm=\"swarm\", error=\"invalid_token\""
        };
        response.headers_mut().insert(
            header::WWW_AUTHENTICATE,
            HeaderValue::from_static(challenge),
        );
        return response;
    }
    let hosts = [
        format!("127.0.0.1:{}", app.port),
        format!("localhost:{}", app.port),
    ];
    if !request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|host| hosts.iter().any(|allowed| allowed == host))
    {
        return error(StatusCode::FORBIDDEN, "host refused").into_response();
    }
    if let Some(origin) = request.headers().get(header::ORIGIN) {
        let valid = origin.to_str().ok().is_some_and(|origin| {
            hosts.iter().any(|host| origin == format!("http://{host}"))
                || app.dev_origin.as_deref() == Some(origin)
        });
        if !valid {
            return error(StatusCode::FORBIDDEN, "origin refused").into_response();
        }
    } else if socket {
        return error(StatusCode::FORBIDDEN, "origin required").into_response();
    }
    // Not through `record_errors`, whose log would keep the ticket in the URI.
    if socket {
        return next.run(request).await;
    }
    if request.method() == Method::POST
        && !request
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value.split(';').next().is_some_and(|mime| {
                    let mime = mime.trim();
                    mime.eq_ignore_ascii_case("application/json")
                        // An image type is not a CORS simple type either, so a page on another
                        // site still cannot send one without a preflight.
                        || (request.uri().path() == "/api/chat/image"
                            && mime.to_ascii_lowercase().starts_with("image/"))
                })
            })
    {
        return error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "JSON content type required",
        )
        .into_response();
    }
    record_errors(&app, request, next).await
}

/// Runs the request and appends each failed reply, with the JSON body that caused it, to
/// `<root>/errors.jsonl`, so an error that the page showed can be diagnosed later.
pub(super) async fn record_errors(app: &App, request: Request, next: Next) -> Response {
    const LIMIT: usize = 64 * 1024;
    let method = request.method().to_string();
    let uri = request.uri().to_string();
    let json_body = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/json"));
    let (request, sent) = if json_body {
        let (parts, body) = request.into_parts();
        let Ok(bytes) = axum::body::to_bytes(body, LIMIT).await else {
            return error(StatusCode::PAYLOAD_TOO_LARGE, "length limit exceeded").into_response();
        };
        (
            Request::from_parts(parts, Body::from(bytes.clone())),
            Some(bytes),
        )
    } else {
        (request, None)
    };
    let response = next.run(request).await;
    if !(response.status().is_client_error() || response.status().is_server_error()) {
        return response;
    }
    let (parts, body) = response.into_parts();
    let reply = axum::body::to_bytes(body, LIMIT).await.unwrap_or_default();
    let value = |bytes: &[u8]| {
        serde_json::from_slice(bytes)
            .unwrap_or_else(|_| Value::String(String::from_utf8_lossy(bytes).into_owned()))
    };
    append_error(
        &app.root,
        json!({
            "source": "server",
            "method": method,
            "uri": uri,
            "status": parts.status.as_u16(),
            "reply": value(&reply),
            "request": sent.as_deref().map(value),
        }),
    );
    Response::from_parts(parts, Body::from(reply))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ShownError {
    pub(super) text: String,
    pub(super) page: String,
}
/// An error that the page showed to the owner, such as a toast.
pub(super) async fn ui_error(State(app): State<App>, Json(shown): Json<ShownError>) -> StatusCode {
    append_error(
        &app.root,
        json!({"source": "ui", "text": shown.text, "page": shown.page}),
    );
    StatusCode::NO_CONTENT
}

// ponytail: the log is never rotated; cap or rotate it if it grows.
pub(super) fn append_error(root: &Path, mut entry: Value) {
    use std::os::unix::fs::OpenOptionsExt;
    entry["at"] = json!(discovery::now());
    let written = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(root.join("errors.jsonl"))
        .and_then(|mut file| writeln!(file, "{entry}"));
    if let Err(err) = written {
        eprintln!("swarm serve: could not record an error: {err}");
    }
}
