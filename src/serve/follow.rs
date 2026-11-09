//! Server follow.

use super::*;

pub(super) async fn flush_transcript(
    app: &Weak<Server>,
    key: &str,
    generation: &str,
    batches: &mut Vec<(u64, Vec<Value>)>,
    source: &std::fs::File,
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
            // Keep the new tail unavailable until its rows frame is queued.
            follower.rendering_rows = follower.rows.is_some();
            if let Some(state) = &mut follower.rows {
                let mut start = follower.from;
                for (end, events) in batches.iter() {
                    view::source_follow::ingest_log_line(state, source, start, *end, events);
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
                follower.rows.as_mut().map(|rows| rows.delta()).map(Ok)
            }
        };
        let (row_frame, changed_rows, restored_rows) = if let Some(pending) = pending {
            if let Ok(delta) = pending {
                let changed = !delta.upsert.is_empty() || !delta.remove.is_empty();
                (Some(row_frame(key, delta)), changed, false)
            } else {
                let state = pending.err().unwrap();
                let render_key = key.to_owned();
                let (mut state, frame, changed) = tokio::task::spawn_blocking(move || {
                    let mut state = state;
                    let delta = state.delta();
                    let changed = !delta.upsert.is_empty() || !delta.remove.is_empty();
                    let frame = row_frame(&render_key, delta);
                    (state, frame, changed)
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
                            break;
                        }
                    };
                    if let Some(incoming) = incoming {
                        let merged = tokio::task::spawn_blocking(move || {
                            state.merge_incoming(incoming);
                            state
                        })
                        .await
                        .map_err(std::io::Error::other)?;
                        state = merged;
                    }
                }
                (Some(frame), changed, true)
            }
        } else {
            (None, false, false)
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
            follower.rendering_rows = false;
            if changed_rows {
                follower.rows_version += 1;
                follower.rows_changed.send_replace(());
            }
            if restored_rows {
                follower.rows_version += 1;
                follower.rows_changed.send_replace(());
            }
            follower.from = batches.last().unwrap().0;
        }
    }
    batches.clear();
    Ok(())
}

pub(super) async fn follow(
    app: Weak<Server>,
    rust: Arc<source_backend::Lease>,
    entry: SubscriptionKey,
    log: Log,
    generation: String,
    mut stop: watch::Receiver<bool>,
    done: watch::Sender<bool>,
) {
    let result = async {
        if *stop.borrow() {
            return Ok(());
        }
        if entry.view.as_deref() == Some("rows") {
            tokio::spawn(watch_turn(
                app.clone(),
                entry.key.clone(),
                generation.clone(),
                stop.clone(),
            ));
        }
        let source = std::fs::File::open(&log.path)?;
        native_follow(&app, &entry, &generation, &source, rust, &mut stop).await
    }
    .await;
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

async fn native_follow(
    app: &Weak<Server>,
    entry: &SubscriptionKey,
    generation: &str,
    source: &std::fs::File,
    rust: Arc<source_backend::Lease>,
    stop: &mut watch::Receiver<bool>,
) -> std::io::Result<()> {
    use swarm::view::transcript::worker_protocol::v2::Operation;
    let mut from = Some(entry.from);
    let mut timer = tokio::time::interval(Duration::from_millis(16));
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            biased;
            _ = stop.changed() => return Ok(()),
            _ = timer.tick() => {
                // Follow runs inline in the worker, so finish this bounded read before stopping.
                let payload = rust.read(Operation::Follow { from: from.take() }).await
                    .map_err(|_| std::io::Error::other("transcript follower ended"))?;
                let mut batches = source_backend::lines(&payload).map_err(|_| std::io::Error::other("invalid transcript output"))?.into_iter().map(|(_,end,events)| (end,events)).collect();
                flush_transcript(app, &entry.key, generation, &mut batches, source).await?;
            }
        }
    }
}

#[cfg(test)]
#[path = "follow/tests.rs"]
mod tests;
