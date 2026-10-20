//! Server row pages.

use super::*;

// Source positions come from the existing follower's per-line page markers, not provider JSON.
pub(super) async fn read_row_window(
    tool: &Path,
    log: &Log,
    start: u64,
    end: u64,
) -> Result<LiveRows, ApiError> {
    let mut state = LiveRows::empty(start, log.format == "codex");
    if start == end {
        return Ok(state);
    }
    let mut child = tokio::process::Command::new(tool)
        .args([
            "--follow",
            "--from",
            &start.to_string(),
            "--format",
            &log.format,
        ])
        .arg(&log.path)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "transcript tool unavailable",
            )
        })?;
    let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
    let read = tokio::time::timeout(Duration::from_secs(10), async {
        let mut first = true;
        let mut cursor = start;
        let mut events = Vec::new();
        loop {
            let line = lines
                .next_line()
                .await
                .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "transcript read failed"))?
                .ok_or_else(|| error(StatusCode::INTERNAL_SERVER_ERROR, "transcript tool ended"))?;
            let value: Value = serde_json::from_str(&line).map_err(|_| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "invalid transcript output",
                )
            })?;
            if value["type"] == "page" {
                if first {
                    first = false;
                    continue;
                }
                let next = value["end_offset"].as_u64().ok_or_else(|| {
                    error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "transcript offset missing",
                    )
                })?;
                if next > end {
                    return Err(error(
                        StatusCode::CONFLICT,
                        "history file changed; reopen this chat",
                    ));
                }
                state.ingest(cursor, next, &events);
                events.clear();
                cursor = next;
                if next == end {
                    return Ok(state);
                }
            } else {
                events.push(value);
            }
        }
    })
    .await
    .map_err(|_| error(StatusCode::GATEWAY_TIMEOUT, "transcript read timed out"));
    let _ = child.kill().await;
    let _ = child.wait().await;
    read?
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
    if query.before.is_none() {
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
        if let Some(state) = cached
            && state.initialized
            && !state.rebuild
            && state.dirty.is_empty()
            && state
                .records
                .first_key_value()
                .is_some_and(|(key, _)| key.0 <= start)
            && state.end == end
        {
            return Ok(Json(
                json!({"page":{"start_offset":start,"end_offset":end},"rows":state.chunks.values().filter(|chunk|state.records.len()<=ROW_RECORD_LIMIT || chunk.records.iter().any(|key|key.0>=start)).flat_map(|chunk|chunk.rows.iter()).chain(state.last_queued.iter()).collect::<Vec<_>>(),"usage":state.usage.value(),"remove":[]}),
            ));
        }
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
            .filter(|state| {
                state
                    .records
                    .first_key_value()
                    .is_some_and(|(key, _)| key.0 > state.start)
            })
            .map(|state| (state.start, state.end))
    } else {
        None
    };
    let recovery = if let Some((start, end)) = recovery {
        Some(read_row_window(&app.tool, &log, start, end).await?)
    } else {
        None
    };
    let page = read_row_window(&app.tool, &log, start, end).await?;
    let key = format!("{}:{}", query.session, query.agent);
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
        state.records.extend(recovery.records);
        state.reindex();
    }
    let previous = state.rows();
    state.records.extend(page.records);
    state.start = state.start.min(start);
    state.end = state.end.max(end);
    state.reindex();
    let rows = state.rows();
    let ids: HashSet<_> = rows.iter().map(|row| &row.id).collect();
    let remove: Vec<_> = previous
        .iter()
        .filter(|row| !ids.contains(&row.id))
        .map(|row| &row.id)
        .collect();
    let usage = state.usage.value();
    let previous: HashMap<_, _> = previous.iter().map(|row| (&row.id, row)).collect();
    let upsert: Vec<_> = rows
        .iter()
        .filter(|row| {
            query.before.is_none() || previous.get(&row.id).is_none_or(|old| *old != *row)
        })
        .collect();
    let response = json!({"page": {"start_offset": start, "end_offset": end}, "rows": upsert, "usage": usage, "remove": remove});
    state.last_usage = usage;
    state.remember_shown(&rows);
    state.prepare(rows.clone());
    state.last_rows = rows;
    state.compact();
    state.last_rows = state.rows();
    let subscribed = hub
        .streams
        .values()
        .any(|client| client.row_keys.contains(&key));
    if subscribed && let Some(follower) = hub.followers.get_mut(&key) {
        follower.rows = Some(state);
    } else {
        hub.cache_rows(key, state);
    }
    Ok(Json(response))
}

#[cfg(test)]
#[path = "row_pages/tests.rs"]
mod tests;
