//! Server follow.

use super::*;

pub(super) async fn flush_transcript(
    app: &Weak<Server>,
    key: &str,
    generation: &str,
    batches: &mut Vec<(u64, Vec<Value>)>,
) -> std::io::Result<()> {
    if batches.is_empty() {
        return Ok(());
    }
    if let Some(app) = app.upgrade() {
        let pending = {
            let mut hub = app.hub.lock().unwrap();
            let Some(follower) = hub
                .followers
                .get_mut(key)
                .filter(|follower| follower.generation == generation)
            else {
                batches.clear();
                return Ok(());
            };
            if let Some(state) = &mut follower.rows {
                let mut start = follower.from;
                for (end, events) in batches.iter() {
                    state.ingest(start, *end, events);
                    start = *end;
                }
            }
            if follower
                .rows
                .as_ref()
                .is_some_and(|state| state.rebuild && state.records.len() > ROW_RECORD_LIMIT)
            {
                follower.rendering_rows = true;
                follower.rows.take().map(Err)
            } else {
                let frame = follower.rows.as_mut().map(|state| state.delta(key));
                frame.map(Ok)
            }
        };
        let row_frame = if let Some(pending) = pending {
            if let Ok(frame) = pending {
                Some(frame)
            } else {
                let state = pending.err().unwrap();
                let render_key = key.to_owned();
                let (mut state, frame) = tokio::task::spawn_blocking(move || {
                    let mut state = state;
                    let frame = state.delta(&render_key);
                    (state, frame)
                })
                .await
                .map_err(std::io::Error::other)?;
                loop {
                    let incoming = {
                        let mut hub = app.hub.lock().unwrap();
                        let Some(follower) = hub
                            .followers
                            .get_mut(key)
                            .filter(|follower| follower.generation == generation)
                        else {
                            batches.clear();
                            return Ok(());
                        };
                        if let Some(incoming) = follower.rows.take() {
                            Some(incoming)
                        } else {
                            follower.rows = Some(state);
                            follower.rendering_rows = false;
                            break;
                        }
                    };
                    if let Some(incoming) = incoming {
                        let merged = tokio::task::spawn_blocking(move || {
                            let usage = state.usage.clone();
                            state.records.extend(incoming.records);
                            state.shown.extend(incoming.shown);
                            state.start = state.start.min(incoming.start);
                            state.end = state.end.max(incoming.end);
                            state.reindex();
                            state.usage = usage;
                            let rows = state.rows_full();
                            state.prepare(rows);
                            state.compact();
                            state
                        })
                        .await
                        .map_err(std::io::Error::other)?;
                        state = merged;
                    }
                }
                Some(frame)
            }
        } else {
            None
        };
        let mut hub = app.hub.lock().unwrap();
        if hub
            .followers
            .get(key)
            .is_none_or(|follower| follower.generation != generation)
        {
            batches.clear();
            return Ok(());
        }
        for client in hub.streams.values_mut() {
            let row_view = client.row_keys.contains(key);
            let Some(cursor) = client.keys.get_mut(key) else {
                continue;
            };
            let mut events = Vec::new();
            let mut end_offset = *cursor;
            for (end, batch) in batches.iter() {
                if *end > end_offset {
                    if !row_view {
                        events.extend(batch.iter().cloned());
                    }
                    end_offset = *end;
                }
            }
            if end_offset != *cursor {
                *cursor = end_offset;
                if row_view {
                    if let Some(frame) = &row_frame {
                        client.push(frame.clone());
                    } else {
                        client.push(Frame::new("reset", json!({"key": key}), Some(key)));
                    }
                    continue;
                }
                client.push(Frame::new(
                    "transcript",
                    json!({"key": key, "events": events, "end_offset": end_offset}),
                    Some(key),
                ));
            }
        }
        if let Some(follower) = hub.followers.get_mut(key) {
            follower.from = batches.last().unwrap().0;
        }
    }
    batches.clear();
    Ok(())
}

pub(super) async fn follow(
    app: Weak<Server>,
    tool: PathBuf,
    entry: SubscriptionKey,
    log: Log,
    generation: String,
    mut stop: watch::Receiver<bool>,
    done: watch::Sender<bool>,
) {
    let result = async {
        if *stop.borrow() { return Ok(()); }
        let mut child = tokio::process::Command::new(tool)
            .args(["--follow", "--from", &entry.from.to_string(), "--format", &log.format])
            .arg(log.path).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::inherit()).kill_on_drop(true).spawn()?;
        let mut lines = BufReader::new(child.stdout.take().unwrap()).lines();
        let mut events = Vec::new();
        let mut batches = Vec::new();
        let mut first = true;
        let mut timer = tokio::time::interval(Duration::from_millis(16));
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let result = async { loop {
            tokio::select! {
                biased;
                _ = stop.changed() => break Ok::<_, std::io::Error>(()),
                _ = timer.tick() => {
                    flush_transcript(&app, &entry.key, &generation, &mut batches).await?;
                    timer.reset();
                }
                line = lines.next_line() => {
                    let Some(line) = line? else { break Err(std::io::Error::other("transcript follower ended")); };
                    let value: Value = serde_json::from_str(&line)?;
                    if value["type"] == "page" {
                        if first { first = false; continue; }
                        let end = value["end_offset"].as_u64().ok_or_else(|| std::io::Error::other("transcript offset missing"))?;
                        batches.push((end, std::mem::take(&mut events)));
                    } else { events.push(value); }
                }
            }
        } }.await;
        let killed = child.kill().await;
        let _ = child.wait().await;
        result.and(killed)
    }.await;
    if let Err(err) = result {
        eprintln!("swarm serve: follower {}: {err}", entry.key);
        if let Some(app) = app.upgrade() {
            let mut hub = app.hub.lock().unwrap();
            if hub
                .followers
                .get(&entry.key)
                .is_some_and(|follower| follower.generation == generation)
            {
                hub.followers.remove(&entry.key);
                for client in hub
                    .streams
                    .values_mut()
                    .filter(|client| client.keys.contains_key(&entry.key))
                {
                    client.push(Frame::new(
                        "reset",
                        json!({"key": entry.key}),
                        Some(&entry.key),
                    ));
                }
            }
        }
    }
    let _ = done.send(true);
}

#[cfg(test)]
#[path = "follow/tests.rs"]
mod tests;
