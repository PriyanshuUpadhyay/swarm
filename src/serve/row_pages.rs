//! Server row pages.

use super::*;

// Source positions come from the held log's Rust worker.
pub(super) async fn read_row_window(
    app: &App,
    key: &str,
    log: &Log,
    start: u64,
    end: u64,
) -> Result<LiveRows, ApiError> {
    source_backend::window(app, key, log, start, end).await
}

pub(super) fn row_offsets(
    path: &Path,
    tail: u16,
    before: Option<u64>,
) -> std::io::Result<(u64, u64)> {
    use std::io::{Seek, SeekFrom};
    let mut file = std::fs::File::open(path)?;
    let mut end = file.metadata()?.len().min(before.unwrap_or(u64::MAX));
    let mut buffer = [0u8; 64 * 1024];
    // A partial line stays with the live follower until its terminating newline arrives.
    let mut cursor = end;
    while cursor > 0 {
        let count = (cursor as usize).min(buffer.len());
        let start = cursor - count as u64;
        file.seek(SeekFrom::Start(start))?;
        file.read_exact(&mut buffer[..count])?;
        if let Some(index) = buffer[..count].iter().rposition(|byte| *byte == b'\n') {
            end = start + index as u64 + 1;
            break;
        }
        cursor = start;
    }
    if cursor == 0 {
        end = 0;
    }
    let mut cursor = end;
    let mut lines = 0;
    while cursor > 0 {
        let count = (cursor as usize).min(buffer.len());
        let start = cursor - count as u64;
        file.seek(SeekFrom::Start(start))?;
        file.read_exact(&mut buffer[..count])?;
        for index in (0..count).rev() {
            let offset = start + index as u64;
            if buffer[index] == b'\n' && offset + 1 != end {
                lines += 1;
                if lines == tail {
                    return Ok((offset + 1, end));
                }
            }
        }
        cursor = start;
    }
    Ok((0, end))
}

pub(super) async fn transcript_rows(
    State(app): State<App>,
    Query(query): Query<TranscriptQuery>,
) -> Result<Json<Value>, ApiError> {
    if !(1..=2000).contains(&query.tail) {
        return Err(error(StatusCode::BAD_REQUEST, "tail must be 1..2000"));
    }
    let root = app.root.clone();
    let session = query.session.clone();
    let agent = query.agent.clone();
    let (log, start, end) = tokio::task::spawn_blocking(move || {
        let log = resolve_log(&root, &session, &agent)?;
        let (start, end) = row_offsets(&log.path, query.tail, query.before)
            .map_err(|_| error(StatusCode::NOT_FOUND, "transcript log unavailable"))?;
        Ok::<_, ApiError>((log, start, end))
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "rows task failed"))??;
    let key = format!("{}:{}", query.session, query.agent);
    let cached_page = if query.before.is_none() {
        let hub = app.hub.lock().unwrap();
        let cached = hub
            .followers
            .get(&key)
            .and_then(|follower| follower.rows.as_ref())
            .or_else(|| {
                hub.row_cache
                    .iter()
                    .find(|(cached, _)| cached == &key)
                    .map(|(_, state)| state)
            });
        cached.and_then(|state| state.cached_tail(start, end))
    } else {
        None
    };
    if let Some(page) = cached_page {
        return tokio::task::spawn_blocking(move || {
            let display = view::markdown::row_displays(&page.rows);
            let patch = view::patch_wire::row_patches(&page.rows);
            let find = view::find_wire::row_finds(&page.rows);
            let images = view::image_paths::row_images(&page.rows);
            Json(json!({
                "page": {"start_offset": start, "end_offset": end},
                "rows": page.rows,
                "usage": page.usage,
                "remove": page.remove,
                "display": display,
            "patch": patch,
                "find": find,
                "images": images,
            }))
        })
        .await
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "rows task failed"));
    }
    let recovery = if query.before.is_some() {
        let hub = app.hub.lock().unwrap();
        hub.followers
            .get(&key)
            .and_then(|follower| follower.rows.as_ref())
            .or_else(|| {
                hub.row_cache
                    .iter()
                    .find(|(cached, _)| cached == &key)
                    .map(|(_, state)| state)
            })
            .and_then(|state| state.recovery_range())
    } else {
        None
    };
    let recovery = if let Some((start, end)) = recovery {
        Some(read_row_window(&app, &key, &log, start, end).await?)
    } else {
        None
    };
    let page = read_row_window(&app, &key, &log, start, end).await?;
    let key = format!("{}:{}", query.session, query.agent);
    let page = {
        let mut hub = app.hub.lock().unwrap();
        let active = hub
            .followers
            .get(&key)
            .and_then(|follower| follower.rows.clone());
        let history = query.before.and_then(|_| {
            hub.row_cache
                .iter()
                .find(|(cached, _)| cached == &key)
                .map(|(_, state)| state.clone())
        });
        let mut state = active.or(history).unwrap_or_else(|| page.clone());
        if let Some(recovery) = recovery {
            state.recover(recovery);
        }
        let page = state.merge_page(page, query.before.is_some());
        let subscribed = hub
            .streams
            .values()
            .any(|client| client.row_keys.contains(&key));
        if subscribed && let Some(follower) = hub.followers.get_mut(&key) {
            follower.rows = Some(state);
        } else {
            hub.cache_rows(key, state);
        }
        page
    };
    tokio::task::spawn_blocking(move || {
        let display = view::markdown::row_displays(&page.rows);
        let patch = view::patch_wire::row_patches(&page.rows);
        let find = view::find_wire::row_finds(&page.rows);
        let images = view::image_paths::row_images(&page.rows);
        Json(json!({
            "page": {"start_offset": start, "end_offset": end},
            "rows": page.rows,
            "usage": page.usage,
            "remove": page.remove,
            "display": display,
            "patch": patch,
            "find": find,
            "images": images,
        }))
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "rows task failed"))
}

#[cfg(test)]
#[path = "row_pages/tests.rs"]
mod tests;
