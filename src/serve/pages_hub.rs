//! Published page copies and their loopback HTTP hub.

use super::*;
use sha2::{Digest, Sha256};
use std::{fs, time::SystemTime};

pub(super) const CSP: &str =
    "sandbox allow-scripts allow-forms allow-popups allow-popups-to-escape-sandbox allow-downloads";

#[derive(Debug)]
pub(super) struct Published {
    pub(super) folder: String,
    pub(super) hash: String,
}

pub(super) fn publish(
    shelf: &Path,
    source: &Path,
    slug: &str,
    date: &str,
) -> Result<Published, String> {
    if slug.is_empty()
        || !slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err("swarm pages: slug must contain only a-z, 0-9 and -".into());
    }
    if date.len() != 10
        || !date.bytes().enumerate().all(|(index, byte)| {
            if index == 4 || index == 7 {
                byte == b'-'
            } else {
                byte.is_ascii_digit()
            }
        })
    {
        return Err("swarm pages: date must have the form YYYY-MM-DD".into());
    }
    refuse_link(source.components().as_path())?;
    let mut files = Vec::new();
    source_files(source, Path::new(""), true, &mut files)?;
    if !files
        .iter()
        .any(|relative| relative == Path::new("index.html"))
    {
        return Err("swarm pages: source needs a top index.html".into());
    }
    files.sort();
    prepare_shelf(shelf).map_err(io_error)?;
    let temporary = shelf.join(format!(".tmp-{}", uuid::Uuid::now_v7()));
    fs::create_dir(&temporary).map_err(io_error)?;
    let folder = format!("{date}-{slug}");
    let target = shelf.join(&folder);
    let result = (|| {
        let mut digest = Sha256::new();
        for relative in files {
            let source_path = source.join(&relative);
            refuse_link(&source_path)?;
            let bytes = fs::read(&source_path).map_err(io_error)?;
            let destination = temporary.join(&relative);
            fs::create_dir_all(destination.parent().unwrap()).map_err(io_error)?;
            fs::write(destination, &bytes).map_err(io_error)?;
            // Lengths keep path/content boundaries unambiguous in the tree hash.
            let name = relative.as_os_str().as_encoded_bytes();
            digest.update((name.len() as u64).to_be_bytes());
            digest.update(name);
            digest.update((bytes.len() as u64).to_be_bytes());
            digest.update(bytes);
        }
        let previous = shelf.join(format!(".tmp-{}", uuid::Uuid::now_v7()));
        let replacing = match fs::symlink_metadata(&target) {
            Ok(metadata) => {
                if metadata.is_symlink() {
                    return Err(link_error(&target));
                }
                fs::rename(&target, &previous).map_err(io_error)?;
                true
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => return Err(io_error(error)),
        };
        if let Err(error) = fs::rename(&temporary, &target) {
            if replacing {
                fs::rename(&previous, &target).map_err(io_error)?;
            }
            return Err(io_error(error));
        }
        if replacing {
            if previous.is_dir() {
                fs::remove_dir_all(previous).map_err(io_error)?;
            } else {
                fs::remove_file(previous).map_err(io_error)?;
            }
        }
        Ok(Published {
            folder,
            hash: format!("{:x}", digest.finalize()),
        })
    })();
    if temporary.exists() {
        let _ = fs::remove_dir_all(&temporary);
    }
    result
}

pub(super) fn prepare_shelf(shelf: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(shelf)?;
    fs::set_permissions(shelf, fs::Permissions::from_mode(0o700))
}

fn io_error(error: std::io::Error) -> String {
    format!("swarm pages: {error}")
}

fn link_error(path: &Path) -> String {
    format!(
        "swarm pages: {} is a symlink; copy the file",
        path.display()
    )
}

fn refuse_link(path: &Path) -> Result<fs::Metadata, String> {
    let metadata = fs::symlink_metadata(path).map_err(io_error)?;
    if metadata.is_symlink() {
        Err(link_error(path))
    } else {
        Ok(metadata)
    }
}

fn source_files(
    source: &Path,
    relative: &Path,
    visible: bool,
    files: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let path = source.join(relative);
    let metadata = refuse_link(&path)?;
    if metadata.is_file() {
        if visible {
            files.push(relative.to_owned());
        }
    } else if metadata.is_dir() {
        for entry in fs::read_dir(path).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            let name = entry.file_name();
            let visible = visible && !name.as_encoded_bytes().starts_with(b".");
            source_files(source, &relative.join(name), visible, files)?;
        }
    }
    Ok(())
}

#[derive(Clone)]
struct Shelf {
    root: PathBuf,
    port: u16,
}

pub(super) fn router(root: PathBuf, port: u16) -> Router {
    Router::new()
        .fallback(shelf)
        .with_state(Shelf { root, port })
}

async fn shelf(State(app): State<Shelf>, request: Request) -> Response {
    let (request, _) = request.into_parts();
    let mut response = shelf_response(&app, &request).await;
    for (name, value) in [
        ("content-security-policy", CSP),
        ("x-content-type-options", "nosniff"),
        ("cache-control", "no-cache"),
        ("referrer-policy", "no-referrer"),
    ] {
        response
            .headers_mut()
            .insert(name, HeaderValue::from_static(value));
    }
    if request.method == Method::HEAD {
        *response.body_mut() = Body::empty();
    }
    response
}

async fn shelf_response(app: &Shelf, request: &axum::http::request::Parts) -> Response {
    let hosts = [
        format!("127.0.0.1:{}", app.port),
        format!("localhost:{}", app.port),
    ];
    if !request
        .headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|host| hosts.iter().any(|allowed| allowed == host))
    {
        return StatusCode::FORBIDDEN.into_response();
    }
    if !matches!(request.method, Method::GET | Method::HEAD) {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    let Some(path) = decode_path(request.uri.path()) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if path.starts_with("//")
        || path.split('/').any(|part| part.starts_with('.'))
        || Path::new(path.trim_start_matches('/'))
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return StatusCode::NOT_FOUND.into_response();
    }
    let Ok(root) = tokio::fs::canonicalize(&app.root).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if path == "/" {
        return bytes_response(index(&root).await.into_bytes(), "text/html; charset=utf-8");
    }
    let relative = path.trim_start_matches('/');
    let Some(mut file) = safe_path(&root, relative).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    if tokio::fs::metadata(&file)
        .await
        .is_ok_and(|metadata| metadata.is_dir())
    {
        let Some(index_file) = safe_path(&root, &format!("{relative}/index.html")).await else {
            return StatusCode::NOT_FOUND.into_response();
        };
        if !path.ends_with('/') {
            let location = format!("{}/", request.uri.path());
            return axum::response::Redirect::temporary(&location).into_response();
        }
        file = index_file;
    }
    let Ok(bytes) = tokio::fs::read(&file).await else {
        return StatusCode::NOT_FOUND.into_response();
    };
    bytes_response(bytes, assets::content_type(&file))
}

fn decode_path(raw: &str) -> Option<String> {
    let mut decoded = Vec::new();
    let mut bytes = raw.bytes();
    while let Some(byte) = bytes.next() {
        decoded.push(if byte == b'%' {
            let high = (bytes.next()? as char).to_digit(16)?;
            let low = (bytes.next()? as char).to_digit(16)?;
            (high * 16 + low) as u8
        } else {
            byte
        });
    }
    let path = String::from_utf8(decoded).ok()?;
    if path.contains(['%', '\0', '\\']) {
        None
    } else {
        Some(path)
    }
}

async fn safe_path(root: &Path, relative: &str) -> Option<PathBuf> {
    let mut path = root.to_owned();
    for part in Path::new(relative).components() {
        let Component::Normal(name) = part else {
            return None;
        };
        if name.as_encoded_bytes().starts_with(b".") {
            return None;
        }
        path.push(name);
        if tokio::fs::symlink_metadata(&path).await.ok()?.is_symlink() {
            return None;
        }
    }
    let canonical = tokio::fs::canonicalize(path).await.ok()?;
    canonical.starts_with(root).then_some(canonical)
}

fn bytes_response(bytes: Vec<u8>, content_type: &'static str) -> Response {
    let length = bytes.len();
    let mut response = Response::new(Body::from(bytes));
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    response.headers_mut().insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&length.to_string()).unwrap(),
    );
    response
}

async fn index(root: &Path) -> String {
    let mut rows = Vec::new();
    if let Ok(mut entries) = tokio::fs::read_dir(root).await {
        while let Ok(Some(entry)) = entries.next_entry().await {
            let Ok(name) = entry.file_name().into_string() else {
                continue;
            };
            if name.starts_with('.') {
                continue;
            }
            let Ok(metadata) = tokio::fs::symlink_metadata(entry.path()).await else {
                continue;
            };
            if !metadata.is_dir() {
                continue;
            }
            let Some(file) = safe_path(root, &format!("{name}/index.html")).await else {
                continue;
            };
            let Ok(bytes) = tokio::fs::read(file).await else {
                continue;
            };
            let html = String::from_utf8_lossy(&bytes);
            let lower = html.to_ascii_lowercase();
            let title = lower
                .find("<title>")
                .and_then(|start| {
                    let start = start + "<title>".len();
                    lower[start..]
                        .find("</title>")
                        .map(|end| html[start..start + end].trim().to_owned())
                })
                .filter(|title| !title.is_empty())
                .unwrap_or_else(|| name.clone());
            let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            rows.push((modified, name, title));
        }
    }
    rows.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| right.1.cmp(&left.1)));
    let mut html = "<!doctype html><html><head><meta charset=\"utf-8\"><title>Swarm pages</title></head><body><h1>Swarm pages</h1><ul>".to_owned();
    for (time, name, title) in rows {
        let seconds = time
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let url: String = name
            .bytes()
            .map(|byte| {
                if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
                    (byte as char).to_string()
                } else {
                    format!("%{byte:02X}")
                }
            })
            .collect();
        html.push_str(&format!("<li>{} <a href=\"/{url}/\">{}</a> <time>{seconds} seconds since 1970-01-01 UTC</time></li>", escape(&title), escape(&name)));
    }
    html.push_str("</ul></body></html>");
    html
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests;
