//! Server row pages.

use super::*;
use swarm::view::transcript::RowKind;

pub(super) const ROWS_FIND_LIMIT: usize = 16 * 1024 * 1024;

pub(super) async fn find_rows(
    body: Result<axum::body::Bytes, axum::extract::rejection::BytesRejection>,
) -> Result<Json<view::find_matches::Reply>, ApiError> {
    let bytes = body.map_err(|rejection| error(rejection.status(), &rejection.body_text()))?;
    tokio::task::spawn_blocking(move || {
        let request: view::find_matches::Request = serde_json::from_slice(&bytes)
            .map_err(|_| error(StatusCode::BAD_REQUEST, "invalid find snapshot"))?;
        if request.version != 1 {
            return Err(error(StatusCode::BAD_REQUEST, "unsupported find version"));
        }
        Ok(Json(view::find_matches::find(request)))
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "find task failed"))?
}

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

#[derive(Deserialize)]
pub(super) struct RowsQuery {
    pub(super) session: String,
    pub(super) agent: String,
    pub(super) tail: u16,
    pub(super) before: Option<u64>,
    pub(super) older_cursor: Option<String>,
}

struct SegmentPage {
    session: String,
    log: Option<Log>,
    start: u64,
    end: u64,
    before: bool,
    older_cursor: Option<String>,
    joins: Vec<(String, String)>,
}

fn invalid_cursor() -> ApiError {
    error(StatusCode::BAD_REQUEST, "invalid older cursor")
}

fn segment_page(root: &Path, query: &RowsQuery) -> Result<SegmentPage, ApiError> {
    let connection = (query.agent == "chair")
        .then(|| read_connection(root))
        .transpose()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "database read failed"))?;
    let chain = if let Some(connection) = &connection {
        swarm::store::continuation_chain(connection, &query.session).map_err(|err| match err {
            rusqlite::Error::QueryReturnedNoRows => error(StatusCode::NOT_FOUND, "unknown session"),
            rusqlite::Error::InvalidQuery => error(StatusCode::CONFLICT, "invalid chat history"),
            _ => error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "chat history read failed",
            ),
        })?
    } else {
        vec![query.session.clone()]
    };
    let linked = chain.len() > 1;
    if (linked && query.before.is_some()) || (!linked && query.older_cursor.is_some()) {
        return Err(invalid_cursor());
    }
    let mut index = chain.len() - 1;
    let mut before = query.before;
    if let Some(cursor) = &query.older_cursor {
        let (source, offset) = cursor.split_once(':').ok_or_else(invalid_cursor)?;
        index = chain
            .iter()
            .position(|id| id == source)
            .ok_or_else(invalid_cursor)?;
        let offset = offset.parse::<u64>().map_err(|_| invalid_cursor())?;
        // At byte zero the next page is at the end of the preceding log.
        if offset == 0 {
            index = index.checked_sub(1).ok_or_else(invalid_cursor)?;
        } else {
            before = Some(offset);
        }
    }
    let mut joins = Vec::new();
    loop {
        let session = &chain[index];
        let log = match resolve_log(root, session, &query.agent) {
            Ok(log) => Some(log),
            Err(err) if linked && err.0 == StatusCode::NOT_FOUND => {
                if before.is_some() {
                    return Err(invalid_cursor());
                }
                None
            }
            Err(err) => return Err(err),
        };
        let (start, end) = match &log {
            Some(log) => row_offsets(&log.path, query.tail, before)
                .map_err(|_| error(StatusCode::NOT_FOUND, "transcript log unavailable"))?,
            None => (0, 0),
        };
        if linked && before.is_some_and(|offset| offset != end) {
            return Err(invalid_cursor());
        }
        if (before.is_none() || start == 0)
            && index > 0
            && let Some(connection) = &connection
        {
            let (from, to): (Option<String>, Option<String>) = connection
                .query_row(
                    "SELECT old.chair_provider, new.chair_provider FROM session AS new
                 JOIN session AS old ON old.id = new.continuation_of WHERE new.id = ?1",
                    [session],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map_err(|_| {
                    error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "chat history read failed",
                    )
                })?;
            if let (Some(from), Some(to)) = (provider_label(from), provider_label(to))
                && from != to
            {
                joins.push((session.clone(), format!("{from} → {to}")));
            }
        }
        if linked && end == 0 && index > 0 {
            index -= 1;
            before = None;
            continue;
        }
        let older_cursor =
            (linked && (start > 0 || index > 0)).then(|| format!("{session}:{start}"));
        return Ok(SegmentPage {
            session: session.clone(),
            log,
            start,
            end,
            before: before.is_some(),
            older_cursor,
            joins,
        });
    }
}

fn provider_label(name: Option<String>) -> Option<&'static str> {
    swarm::providers::Provider::parse(name.as_deref()?).map(|provider| provider.info().label)
}

pub(super) async fn transcript_rows(
    State(app): State<App>,
    Query(query): Query<RowsQuery>,
) -> Result<Json<Value>, ApiError> {
    if !(1..=2000).contains(&query.tail) {
        return Err(error(StatusCode::BAD_REQUEST, "tail must be 1..2000"));
    }
    let root = app.root.clone();
    let session = query.session.clone();
    let agent = query.agent.clone();
    let segment = tokio::task::spawn_blocking(move || segment_page(&root, &query))
        .await
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "rows task failed"))??;
    let (page, model) = if let Some(log) = &segment.log {
        load_segment(&app, &agent, &segment, log).await?
    } else {
        (
            view::reader_rows::RowPage {
                rows: Vec::new(),
                remove: Vec::new(),
                usage: None,
            },
            None,
        )
    };
    render_row_page(page, segment, &session, model).await
}

async fn load_segment(
    app: &App,
    agent: &str,
    segment: &SegmentPage,
    log: &Log,
) -> Result<(view::reader_rows::RowPage, Option<String>), ApiError> {
    let SegmentPage {
        start, end, before, ..
    } = *segment;
    let key = format!("{}:{agent}", segment.session);
    let needs_model = segment
        .joins
        .iter()
        .any(|(source, _)| source == &segment.session);
    let model = |state: &LiveRows| needs_model.then(|| state.first_model()).flatten();
    let cached_page = if !before {
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
        cached.and_then(|state| {
            state
                .cached_tail(start, end)
                .map(|page| (page, model(state)))
        })
    } else {
        None
    };
    if let Some(page) = cached_page {
        return Ok(page);
    }
    let recovery = if before {
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
        Some(read_row_window(app, &key, log, start, end).await?)
    } else {
        None
    };
    let page = read_row_window(app, &key, log, start, end).await?;
    let page = {
        let mut hub = app.hub.lock().unwrap();
        let active = hub
            .followers
            .get(&key)
            .and_then(|follower| follower.rows.clone());
        let history = before
            .then(|| {
                hub.row_cache
                    .iter()
                    .find(|(cached, _)| cached == &key)
                    .map(|(_, state)| state.clone())
            })
            .flatten();
        let mut state = active.or(history).unwrap_or_else(|| page.clone());
        if let Some(recovery) = recovery {
            state.recover(recovery);
        }
        let page = state.merge_page(page, before);
        let subscribed = hub
            .streams
            .values()
            .any(|client| client.row_keys.contains(&key));
        let model = model(&state);
        if subscribed && let Some(follower) = hub.followers.get_mut(&key) {
            follower.rows = Some(state);
        } else {
            hub.cache_rows(key, state);
        }
        (page, model)
    };
    Ok(page)
}

async fn render_row_page(
    mut page: view::reader_rows::RowPage,
    segment: SegmentPage,
    leaf: &str,
    model: Option<String>,
) -> Result<Json<Value>, ApiError> {
    let leaf = leaf.to_owned();
    let historical = segment.session != leaf;
    tokio::task::spawn_blocking(move || {
        let (start, end) = if historical {
            page.rows.retain(|row| row.kind != RowKind::Queued);
            for row in &mut page.rows {
                row.scope_to(&segment.session);
            }
            for id in &mut page.remove {
                *id = format!("{}:{id}", segment.session);
            }
            page.usage = None;
            (0, 0)
        } else {
            (segment.start, segment.end)
        };
        for (source, mut label) in segment.joins.into_iter().rev() {
            if source == segment.session && let Some(model) = &model {
                label.push_str(&format!(" · {model}"));
            }
            let mut row = Row::new(RowKind::ModelChange, label, format!("cli-join:{source}"));
            if source != leaf {
                row.scope_to(&source);
            }
            if source == segment.session {
                page.rows.insert(0, row);
            } else {
                page.rows.push(row);
            }
        }
        let display = view::markdown::row_displays(&page.rows);
        let body = view::markdown::row_bodies(&page.rows, &display);
        let patch = view::patch_wire::row_patches(&page.rows);
        let find = view::find_wire::row_finds(&page.rows);
        let images = view::image_paths::row_images(&page.rows);
        Json(json!({
            "page": {"start_offset": start, "end_offset": end, "older_cursor": segment.older_cursor},
            "rows": page.rows,
            "usage": page.usage,
            "remove": page.remove,
            "display": display,
            "body": body,
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
