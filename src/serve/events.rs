//! Server events.

use super::*;

pub(super) struct Events {
    pub(super) app: App,
    pub(super) id: String,
}

impl Stream for Events {
    type Item = Result<Event, Infallible>;
    fn poll_next(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let mut hub = self.app.hub.lock().unwrap();
        let Some(client) = hub.streams.get_mut(&self.id) else {
            return Poll::Ready(None);
        };
        if let Some(frame) = client.queue.pop_front() {
            Poll::Ready(Some(Ok(Event::default()
                .event(frame.name)
                .data(frame.data))))
        } else {
            client.waker = Some(context.waker().clone());
            Poll::Pending
        }
    }
}

impl Drop for Events {
    fn drop(&mut self) {
        let mut hub = self.app.hub.lock().unwrap();
        hub.streams.remove(&self.id);
        hub.prune();
    }
}

pub(super) async fn stream(State(app): State<App>) -> Sse<Events> {
    let id = uuid::Uuid::now_v7().to_string();
    let mut client = Client {
        queue: VecDeque::new(),
        waker: None,
        keys: HashMap::new(),
        row_keys: HashSet::new(),
    };
    client.push(Frame::new(
        "hello",
        json!({"stream": id, "build": BUILD}),
        None,
    ));
    app.hub.lock().unwrap().streams.insert(id.clone(), client);
    Sse::new(Events { app, id })
}

/// `swarm ui` reloads the open pages, so they show this server's build.
pub(super) async fn reload(State(app): State<App>) -> Json<Value> {
    let mut hub = app.hub.lock().unwrap();
    hub.publish(Frame::new("reload", json!({}), None));
    Json(json!({"pages": hub.streams.len()}))
}

#[derive(Deserialize)]
pub(super) struct Show {
    pub(super) session: String,
}

/// ADR 0102: a notice click asks the open pages to open one chat.
pub(super) async fn show(
    State(app): State<App>,
    body: Result<Json<Show>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    let Json(body) = body.map_err(|e| error(e.status(), &e.body_text()))?;
    if !valid_session(&body.session) {
        return Err(error(StatusCode::BAD_REQUEST, "bad session id"));
    }
    let mut hub = app.hub.lock().unwrap();
    hub.publish(Frame::new("show", json!({"session": body.session}), None));
    Ok(Json(json!({"pages": hub.streams.len()})))
}

#[derive(Deserialize)]
pub(super) struct Subscription {
    pub(super) stream: String,
    pub(super) keys: Vec<SubscriptionKey>,
}
#[derive(Deserialize)]
pub(super) struct SubscriptionKey {
    pub(super) key: String,
    pub(super) from: u64,
    #[serde(default)]
    pub(super) view: Option<String>,
}

pub(super) async fn subscribe(
    State(app): State<App>,
    Json(body): Json<Subscription>,
) -> Result<Json<Value>, ApiError> {
    if body.keys.len() > 32 {
        return Err(error(StatusCode::BAD_REQUEST, "at most 32 keys"));
    }
    let root = app.root.clone();
    let keys = tokio::task::spawn_blocking(move || {
        let mut unique = HashSet::new();
        let mut keys = Vec::new();
        for mut entry in body.keys {
            if entry.view.as_deref().is_some_and(|view| view != "rows") {
                return Err(error(StatusCode::BAD_REQUEST, "unknown transcript view"));
            }
            let (session, agent) = entry
                .key
                .split_once(':')
                .ok_or_else(|| error(StatusCode::BAD_REQUEST, "invalid transcript key"))?;
            if !unique.insert(entry.key.clone()) {
                return Err(error(StatusCode::BAD_REQUEST, "duplicate transcript key"));
            }
            let log = resolve_log(&root, session, agent)?;
            entry.from = entry.from.min(
                std::fs::metadata(&log.path)
                    .map_err(|_| error(StatusCode::NOT_FOUND, "transcript log unavailable"))?
                    .len(),
            );
            keys.push((entry, log));
        }
        Ok::<_, ApiError>((body.stream, keys))
    })
    .await
    .map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "subscription task failed",
        )
    })??;
    let (id, keys) = keys;
    let mut seeds = HashMap::new();
    for (entry, log) in &keys {
        if entry.view.as_deref() != Some("rows") {
            continue;
        }
        let known = {
            let hub = app.hub.lock().unwrap();
            hub.followers
                .get(&entry.key)
                .is_some_and(|follower| follower.rows.is_some() || follower.rendering_rows)
                || hub.row_cache.iter().any(|(key, _)| key == &entry.key)
        };
        if !known {
            let path = log.path.clone();
            let from = entry.from;
            let (start, end) =
                tokio::task::spawn_blocking(move || row_offsets(&path, 200, Some(from)))
                    .await
                    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "rows seed task failed"))?
                    .map_err(|_| error(StatusCode::NOT_FOUND, "transcript log unavailable"))?;
            let mut seed = read_row_window(&app.tool, log, start, end).await?;
            seed.last_rows = seed.rows();
            seed.prepare(seed.last_rows.clone());
            seeds.insert(entry.key.clone(), seed);
        }
    }
    let mut launch = Vec::new();
    let mut wait = Vec::new();
    {
        let mut hub = app.hub.lock().unwrap();
        let client = hub
            .streams
            .get_mut(&id)
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "unknown stream"))?;
        client.keys = keys
            .iter()
            .map(|(entry, _)| (entry.key.clone(), entry.from))
            .collect();
        client.row_keys = keys
            .iter()
            .filter(|(entry, _)| entry.view.as_deref() == Some("rows"))
            .map(|(entry, _)| entry.key.clone())
            .collect();
        client.queue.retain(|frame| frame.key.is_none());
        hub.prune();
        for (entry, log) in keys {
            let needs_rows = hub
                .streams
                .values()
                .any(|client| client.row_keys.contains(&entry.key));
            let active_seed = hub
                .followers
                .get(&entry.key)
                .and_then(|follower| follower.rows.clone());
            let seed = active_seed
                .or_else(|| {
                    hub.row_cache
                        .iter()
                        .position(|(key, _)| key == &entry.key)
                        .map(|index| hub.row_cache.remove(index).unwrap().1)
                })
                .or_else(|| seeds.remove(&entry.key));
            if let Some(follower) = hub.followers.get_mut(&entry.key)
                && needs_rows
                && follower.rows.is_none()
            {
                follower.rows = seed.clone();
            }
            if let Some(done) = hub.retiring.remove(&entry.key) {
                wait.push(done);
            }
            if hub
                .followers
                .get(&entry.key)
                .is_some_and(|follower| follower.from <= entry.from)
            {
                continue;
            }
            if let Some(old) = hub.followers.remove(&entry.key) {
                let _ = old.stop.send(true);
                wait.push(old.done);
            }
            let (stop, receiver) = watch::channel(false);
            let (done_sender, done) = watch::channel(false);
            let generation = uuid::Uuid::now_v7().to_string();
            hub.followers.insert(
                entry.key.clone(),
                Follower {
                    generation: generation.clone(),
                    from: entry.from,
                    stop,
                    done,
                    rendering_rows: false,
                    rows: if needs_rows {
                        seed.or_else(|| Some(LiveRows::empty(entry.from, log.format == "codex")))
                    } else {
                        None
                    },
                },
            );
            launch.push((entry, log, generation, receiver, done_sender));
        }
    }
    for mut done in wait {
        let _ = done.wait_for(|finished| *finished).await;
    }
    for (entry, log, generation, stop, done) in launch {
        tokio::spawn(follow(
            Arc::downgrade(&app),
            app.tool.clone(),
            entry,
            log,
            generation,
            stop,
            done,
        ));
    }
    Ok(Json(json!({})))
}

#[cfg(test)]
#[path = "events/tests.rs"]
mod tests;
