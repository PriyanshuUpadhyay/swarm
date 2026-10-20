//! Server images.

use super::*;

pub(super) const IMAGE_LIMIT: usize = 10 * 1024 * 1024;

/// A pasted image, saved under `<swarm home>/uploads` with mode 0600. The composer puts the
/// returned path in the message; Claude, Codex and pi each open an image by its path.
// ponytail: uploads are never removed; prune by age if the folder grows.
pub(super) async fn chat_image(
    State(app): State<App>,
    headers: axum::http::HeaderMap,
    body: axum::body::Bytes,
) -> Result<Json<Value>, ApiError> {
    let kind = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    let (ext, _) = IMAGE_TYPES
        .iter()
        .find(|(_, mime)| *mime == kind)
        .ok_or_else(|| {
            error(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "Paste a PNG, JPEG, GIF or WebP image",
            )
        })?;
    if body.is_empty() {
        return Err(error(StatusCode::BAD_REQUEST, "The image is empty"));
    }
    let dir = app.root.join("uploads");
    let path = dir.join(format!("{}.{ext}", uuid::Uuid::now_v7()));
    let saved = path.clone();
    tokio::task::spawn_blocking(move || {
        use std::{
            io::Write,
            os::unix::fs::{DirBuilderExt, OpenOptionsExt},
        };
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&dir)?;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&saved)?
            .write_all(&body)
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "image task failed"))?
    .map_err(|e| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("Could not save the image: {e}"),
        )
    })?;
    Ok(Json(json!({"path": path.to_string_lossy()})))
}

pub(super) const IMAGE_TYPES: [(&str, &str); 4] = [
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
];

#[derive(Deserialize)]
pub(super) struct ImageName {
    pub(super) name: String,
}

/// A pasted image for its preview. Only a file name that the paste route made is read, so no
/// other file under the swarm home can be named.
pub(super) async fn chat_image_read(
    State(app): State<App>,
    Query(query): Query<ImageName>,
) -> Result<Response, ApiError> {
    let (stem, ext) = query.name.rsplit_once('.').unwrap_or_default();
    let mime = IMAGE_TYPES
        .iter()
        .find(|(known, _)| *known == ext)
        .map(|(_, mime)| *mime)
        .filter(|_| uuid::Uuid::try_parse(stem).is_ok() && stem.len() == 36)
        .ok_or_else(|| error(StatusCode::BAD_REQUEST, "not a pasted image name"))?;
    let path = app.root.join("uploads").join(&query.name);
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|_| error(StatusCode::NOT_FOUND, "The image is gone"))?;
    Ok((
        [
            (header::CONTENT_TYPE, mime),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::CACHE_CONTROL, "private, max-age=86400"),
        ],
        bytes,
    )
        .into_response())
}

#[cfg(test)]
#[path = "images/tests.rs"]
mod tests;
