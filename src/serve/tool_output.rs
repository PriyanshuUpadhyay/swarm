//! Server tool output.

use super::*;

#[derive(Deserialize)]
pub(super) struct ToolOutputQuery {
    pub(super) session: String,
    pub(super) agent: String,
    pub(super) row: String,
}
pub(super) fn output_locator(rows: &[Row], id: &str) -> Option<String> {
    for row in rows {
        if row.id == id {
            return row
                .tool
                .as_ref()
                .and_then(|tool| tool.output.as_ref())
                .map(|output| output.id.clone())
                .or_else(|| row.diff.as_ref().map(|diff| diff.id.clone()))
                .or_else(|| {
                    row.tool
                        .as_ref()
                        .and_then(|tool| tool.diffs.first())
                        .map(|diff| diff.id.clone())
                })
                .or_else(|| row.shell.as_ref().map(|shell| shell.output.id.clone()))
                .or_else(|| {
                    row.command
                        .as_ref()
                        .and_then(|command| command.output.as_ref())
                        .map(|output| output.id.clone())
                })
                .or_else(|| row.output.as_ref().map(|output| output.id.clone()));
        }
        if let Some(id) = output_locator(&row.steps, id) {
            return Some(id);
        }
    }
    None
}

pub(super) async fn tool_output(
    State(app): State<App>,
    Query(mut query): Query<ToolOutputQuery>,
) -> Result<Json<Value>, ApiError> {
    {
        let key = format!("{}:{}", query.session, query.agent);
        let hub = app.hub.lock().unwrap();
        let state = hub
            .followers
            .get(&key)
            .and_then(|follower| follower.rows.as_ref())
            .or_else(|| {
                hub.row_cache
                    .iter()
                    .find(|(cached, _)| cached == &key)
                    .map(|(_, state)| state)
            });
        if let Some(state) = state
            && let Some(locator) = state
                .chunks
                .values()
                .find_map(|chunk| output_locator(&chunk.rows, &query.row))
                .or_else(|| output_locator(&state.last_rows, &query.row))
        {
            query.row = locator;
        }
    }
    if let Some((_, source)) = query.row.rsplit_once('@') {
        let offset = source
            .strip_prefix("raw-")
            .and_then(|source| source.split('-').next())
            .and_then(|offset| offset.parse::<u64>().ok())
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "unknown tool output"))?;
        let log = resolve_log(&app.root, &query.session, &query.agent)?;
        let path = log.path.clone();
        let end = tokio::task::spawn_blocking(move || {
            use std::io::{BufRead, Seek, SeekFrom};
            let mut file = std::fs::File::open(path)?;
            file.seek(SeekFrom::Start(offset))?;
            let mut reader = std::io::BufReader::new(file);
            let mut line = String::new();
            let mut end = offset;
            for _ in 0..ROW_RECORD_LIMIT {
                line.clear();
                let bytes = reader.read_line(&mut line)?;
                if bytes == 0 {
                    break;
                }
                end += bytes as u64;
            }
            Ok::<_, std::io::Error>(end)
        })
        .await
        .map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "tool locator task failed",
            )
        })?
        .map_err(|_| error(StatusCode::NOT_FOUND, "unknown tool output"))?;
        let mut window = read_row_window(&app.tool, &log, offset, end).await?;
        if output_locator(&window.rows(), &query.row).is_none() {
            let full_end = std::fs::metadata(&log.path)
                .map_err(|_| error(StatusCode::NOT_FOUND, "unknown tool output"))?
                .len();
            if end < full_end {
                window = read_row_window(&app.tool, &log, offset, full_end).await?;
            }
        }
        query.row = output_locator(&window.rows(), &query.row)
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "unknown tool output"))?;
    }
    let (source, component) = query
        .row
        .rsplit_once(':')
        .filter(|(_, component)| matches!(*component, "output" | "diff"))
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "unknown tool output"))?;
    let mut pieces = source
        .strip_prefix("raw-")
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "unknown tool output"))?
        .split('-');
    let offset: u64 = pieces
        .next()
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "unknown tool output"))?;
    let index: usize = pieces
        .next()
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "unknown tool output"))?;
    if pieces.next().is_some() {
        return Err(error(StatusCode::NOT_FOUND, "unknown tool output"));
    }
    let component = component.to_owned();
    let text = tokio::task::spawn_blocking(move || {
        use std::io::{BufRead, Seek, SeekFrom};
        use std::os::unix::fs::OpenOptionsExt;
        let log = resolve_log(&app.root, &query.session, &query.agent)?;
        let mut file = std::fs::File::open(&log.path)
            .map_err(|_| error(StatusCode::NOT_FOUND, "transcript log unavailable"))?;
        if offset > 0 {
            file.seek(SeekFrom::Start(offset - 1))
                .map_err(|_| error(StatusCode::NOT_FOUND, "unknown tool output"))?;
            let mut newline = [0];
            file.read_exact(&mut newline)
                .map_err(|_| error(StatusCode::NOT_FOUND, "unknown tool output"))?;
            if newline != *b"\n" {
                return Err(error(StatusCode::NOT_FOUND, "unknown tool output"));
            }
        }
        let mut reader = std::io::BufReader::new(file);
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .map_err(|_| error(StatusCode::NOT_FOUND, "unknown tool output"))?;
        if line.is_empty() {
            return Err(error(StatusCode::NOT_FOUND, "unknown tool output"));
        }
        // The translator still owns parsing. A private one-line copy keeps on-demand work bounded.
        let temporary =
            std::env::temp_dir().join(format!("swarm-output-{}.jsonl", uuid::Uuid::now_v7()));
        let scratch = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .and_then(|mut file| file.write_all(line.as_bytes()));
        if scratch.is_err() {
            let _ = std::fs::remove_file(&temporary);
            return Err(error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "output scratch write failed",
            ));
        }
        let output = std::process::Command::new(&app.tool)
            .args(["--format", &log.format])
            .arg(&temporary)
            .output();
        let _ = std::fs::remove_file(temporary);
        let output = output.map_err(|_| {
            error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "transcript tool unavailable",
            )
        })?;
        if !output.status.success() {
            return Err(error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "transcript tool failed",
            ));
        }
        let events: Vec<Value> = output
            .stdout
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .map(serde_json::from_slice)
            .collect::<Result<_, _>>()
            .map_err(|_| {
                error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "invalid transcript output",
                )
            })?;
        let event = events
            .get(index)
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "unknown tool output"))?;
        let text = match (view::string(event, "type"), component.as_str()) {
            ("tool_call_update", "output") => view::string(event, "content").to_owned(),
            ("tool_diff", "diff") => {
                swarm::view::diff_patch(
                    &serde_json::from_value::<swarm::view::Diff>(event.clone())
                        .map_err(|_| error(StatusCode::NOT_FOUND, "unknown tool output"))?,
                    true,
                )
                .0
            }
            ("system_message", "output") => match view::string(event, "kind") {
                "shell_output" => view::shell_output_text(view::string(event, "text")),
                "command_output" => view::command_output_text(view::string(event, "text")),
                _ => view::string(event, "text").to_owned(),
            },
            // A record row: an ignored, unknown or bookkeeping event.
            (_, "output") => view::record_text(event),
            _ => return Err(error(StatusCode::NOT_FOUND, "unknown tool output")),
        };
        Ok(Json(json!({"text": text})))
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "tool output task failed"))??;
    Ok(text)
}

#[cfg(test)]
#[path = "tool_output/tests.rs"]
mod tests;
