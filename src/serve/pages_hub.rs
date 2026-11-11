//! Published page copies and their loopback HTTP hub.

use super::*;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{fs, time::SystemTime};

pub(super) const CSP: &str =
    "sandbox allow-scripts allow-forms allow-popups allow-popups-to-escape-sandbox allow-downloads";

#[derive(Debug)]
pub(super) struct Published {
    pub(super) id: String,
    pub(super) hash: String,
}

/// A page's own record, kept as a dot file so the hub never serves it.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct Page {
    #[serde(skip)]
    pub(super) id: String,
    pub(super) date: String,
    pub(super) slug: String,
    pub(super) source: String,
}

const META: &str = ".page.json";

/// A page folder name is a lowercase hyphenated UUID v4, so a URL cannot be guessed (ADR 0141).
pub(super) fn page_id(name: &str) -> bool {
    uuid::Uuid::try_parse(name)
        .is_ok_and(|id| id.get_version_num() == 4 && id.hyphenated().to_string() == name)
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
    let origin = fs::canonicalize(source)
        .map_err(io_error)?
        .to_string_lossy()
        .into_owned();
    prepare_shelf(shelf).map_err(io_error)?;
    // A second publish of the same source keeps its ID, so a shared link stays valid.
    let id = list(shelf)?
        .into_iter()
        .find(|page| page.source == origin)
        .map_or_else(|| uuid::Uuid::new_v4().to_string(), |page| page.id);
    let temporary = shelf.join(format!(".tmp-{}", uuid::Uuid::now_v7()));
    fs::create_dir(&temporary).map_err(io_error)?;
    let target = shelf.join(&id);
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
        let page = Page {
            id: id.clone(),
            date: date.into(),
            slug: slug.into(),
            source: origin,
        };
        write_meta(&temporary, &page)?;
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
            id,
            hash: format!("{:x}", digest.finalize()),
        })
    })();
    if temporary.exists() {
        let _ = fs::remove_dir_all(&temporary);
    }
    result
}

/// Makes the shelf private and gives each page from before ADR 0141 a random ID.
pub(super) fn prepare_shelf(shelf: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(shelf)?;
    fs::set_permissions(shelf, fs::Permissions::from_mode(0o700))?;
    for entry in fs::read_dir(shelf)? {
        let entry = entry?;
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if name.starts_with('.') || page_id(&name) || !entry.file_type()?.is_dir() {
            continue;
        }
        let (date, slug) = match name.get(..10).zip(name.get(11..)) {
            Some((date, slug)) if name.as_bytes()[10] == b'-' => (date, slug),
            _ => ("", name.as_str()),
        };
        let page = Page {
            id: String::new(),
            date: date.into(),
            slug: slug.into(),
            source: String::new(),
        };
        write_meta(&entry.path(), &page).map_err(std::io::Error::other)?;
        fs::rename(entry.path(), shelf.join(uuid::Uuid::new_v4().to_string()))?;
    }
    Ok(())
}

fn write_meta(folder: &Path, page: &Page) -> Result<(), String> {
    let text = serde_json::to_string(page).map_err(|error| format!("swarm pages: {error}"))?;
    fs::write(folder.join(META), text).map_err(io_error)
}

/// The pages on the shelf, newest first. Only the owner's CLI and app read this list.
pub(super) fn list(shelf: &Path) -> Result<Vec<Page>, String> {
    let entries = match fs::read_dir(shelf) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(io_error(error)),
    };
    let mut rows = Vec::new();
    for entry in entries {
        let entry = entry.map_err(io_error)?;
        let Ok(id) = entry.file_name().into_string() else {
            continue;
        };
        let Ok(metadata) = fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if !page_id(&id)
            || !metadata.is_dir()
            || !fs::symlink_metadata(entry.path().join("index.html"))
                .is_ok_and(|file| file.is_file())
        {
            continue;
        }
        let mut page = fs::read_to_string(entry.path().join(META))
            .ok()
            .and_then(|text| serde_json::from_str::<Page>(&text).ok())
            .unwrap_or(Page {
                id: String::new(),
                date: String::new(),
                slug: String::new(),
                source: String::new(),
            });
        page.id = id;
        rows.push((metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH), page));
    }
    rows.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| right.1.id.cmp(&left.1.id))
    });
    Ok(rows.into_iter().map(|(_, page)| page).collect())
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
    /// With `--id`, the hub serves only these pages.
    only: Option<Vec<String>>,
}

pub(super) fn router(root: PathBuf, port: u16, only: Option<Vec<String>>) -> Router {
    Router::new()
        .fallback(shelf)
        .with_state(Shelf { root, port, only })
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
    response.headers_mut().insert(
        "x-swarm-pages-pid",
        HeaderValue::from_str(&std::process::id().to_string()).unwrap(),
    );
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
    let relative = path.trim_start_matches('/');
    // No listing: a page is reached only through its ID.
    let id = relative.split('/').next().unwrap_or_default();
    if !page_id(id)
        || app
            .only
            .as_ref()
            .is_some_and(|only| !only.iter().any(|allowed| allowed == id))
    {
        return StatusCode::NOT_FOUND.into_response();
    }
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

#[cfg(test)]
mod tests;
