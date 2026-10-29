//! HTTP server state and route modules.

use axum::{
    Json, Router,
    body::Body,
    extract::{DefaultBodyLimit, Query, Request, State, rejection::JsonRejection},
    http::{HeaderValue, Method, StatusCode, Uri, header},
    middleware::{self, Next},
    response::{IntoResponse, Response, Sse, sse::Event},
    routing::{get, post},
};
use rusqlite::OptionalExtension;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    convert::Infallible,
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    pin::Pin,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, Waker},
    time::Duration,
};
use swarm::view::transcript::{self as view, Record, Row};
use swarm::view::{
    changes, choices, choices_store, claude_text, discovery, fields, handoff, lifecycle,
    navigation, queue, tabs, tree, workspace_panel,
};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    sync::{mpsc, oneshot, watch},
};
use tokio_stream::Stream;

const QUEUE_LIMIT: usize = 1024;
type ApiError = (StatusCode, Json<Value>);
type App = Arc<Server>;

fn error(status: StatusCode, message: &str) -> ApiError {
    (status, Json(json!({"error": message})))
}

struct Server {
    root: PathBuf,
    tool: PathBuf,
    port: u16,
    /// A `swarm ui` server, which writes its record; only it can restart on a new build.
    ui: bool,
    token: String,
    assets: Option<PathBuf>,
    dev_origin: Option<String>,
    hub: Mutex<Hub>,
    directories: Mutex<HashMap<String, (Option<String>, String)>>,
    stopped: AtomicBool,
    cli: PathBuf,
    actions: mpsc::Sender<ActionJob>,
    tree_cache: Mutex<TreeCache>,
    handoffs: Mutex<HashMap<String, HandoffRun>>,
    chat_create: tokio::sync::Mutex<()>,
    // shortcut: the cooldown resets on server restart; seed from cache if restart bursts matter.
    usage_refresh: Mutex<HashMap<String, (tokio::time::Instant, Option<ApiError>)>>,
    commands: Mutex<HashMap<(String, String), Arc<CommandsSlot>>>,
}

/// One agent's `/` menu list: the last answer and when it goes stale, and the lock its one
/// running probe holds.
#[derive(Default)]
struct CommandsSlot {
    last: Mutex<Option<(tokio::time::Instant, Value)>>,
    probe: tokio::sync::Mutex<()>,
}

#[derive(Default)]
struct TreeCache {
    discovery: discovery::Discovery,
    snapshot: Value,
    revision: String,
    saved: Option<choices_store::Saved>,
    choices_error: Option<String>,
}

struct Hub {
    version: u64,
    agents: Value,
    streams: HashMap<String, Client>,
    followers: HashMap<String, Follower>,
    retiring: HashMap<String, watch::Receiver<bool>>,
    row_cache: VecDeque<(String, LiveRows)>,
}

struct Client {
    queue: VecDeque<Frame>,
    waker: Option<Waker>,
    keys: HashMap<String, u64>,
    row_keys: HashSet<String>,
}

#[derive(Clone)]
struct Frame {
    name: &'static str,
    data: String,
    key: Option<String>,
}

impl Frame {
    fn new(name: &'static str, data: Value, key: Option<&str>) -> Self {
        Self {
            name,
            data: data.to_string(),
            key: key.map(str::to_owned),
        }
    }
}

impl Client {
    fn push(&mut self, mut frame: Frame) {
        if frame.name == "snapshot" {
            self.queue.retain(|queued| queued.name != "snapshot");
        }
        if self.queue.len() == QUEUE_LIMIT {
            let keys: HashSet<_> = self
                .queue
                .iter()
                .filter_map(|queued| queued.key.clone())
                .collect();
            self.queue
                .retain(|queued| queued.key.is_none() && queued.name != "ping");
            for key in keys {
                self.queue
                    .push_back(Frame::new("reset", json!({"key": key}), Some(&key)));
            }
            if let Some(key) = frame.key.as_deref() {
                self.queue
                    .retain(|queued| queued.key.as_deref() != Some(key));
                frame = Frame::new("reset", json!({"key": key}), Some(key));
            }
        }
        self.queue.push_back(frame);
        if let Some(waker) = self.waker.take() {
            waker.wake();
        }
    }
}

struct Follower {
    generation: String,
    from: u64,
    stop: watch::Sender<bool>,
    done: watch::Receiver<bool>,
    rows: Option<LiveRows>,
    rendering_rows: bool,
}

impl Hub {
    fn cache_rows(&mut self, key: String, rows: LiveRows) {
        self.row_cache.retain(|(cached, _)| cached != &key);
        self.row_cache.push_back((key, rows));
        while self.row_cache.len() > 4 {
            self.row_cache.pop_front();
        }
    }
    fn bump(&mut self) {
        self.version += 1;
        let frame = Frame::new("snapshot", json!({"version": self.version}), None);
        self.publish(frame);
    }
    fn publish(&mut self, frame: Frame) {
        for stream in self.streams.values_mut() {
            stream.push(frame.clone());
        }
    }

    fn prune(&mut self) {
        let rowless: Vec<_> = self
            .followers
            .keys()
            .filter(|key| {
                !self
                    .streams
                    .values()
                    .any(|stream| stream.row_keys.contains(*key))
            })
            .cloned()
            .collect();
        for key in rowless {
            if let Some(rows) = self
                .followers
                .get_mut(&key)
                .and_then(|follower| follower.rows.take())
            {
                self.cache_rows(key, rows);
            }
        }
        self.retiring
            .retain(|_, done| !*done.borrow() && done.has_changed().is_ok());
        let unused: Vec<_> = self
            .followers
            .keys()
            .filter(|key| {
                !self
                    .streams
                    .values()
                    .any(|stream| stream.keys.contains_key(*key))
            })
            .cloned()
            .collect();
        for key in unused {
            let follower = self.followers.remove(&key).unwrap();
            let _ = follower.stop.send(true);
            if let Some(rows) = follower.rows {
                self.cache_rows(key.clone(), rows);
            }
            self.retiring.insert(key, follower.done);
        }
    }
}

mod action_execute;
use action_execute::*;

mod actions;
use actions::*;

mod assets;
use assets::*;

mod background;
use background::*;

mod browser;
use browser::*;

mod choice_routes;
use choice_routes::*;

mod cli_process;
use cli_process::*;

mod commands;
use commands::*;

mod continue_chat;
use continue_chat::*;

mod coordinator;
use coordinator::*;

mod events;
use events::*;

mod follow;
use follow::*;

mod guards;
use guards::*;

mod images;
use images::*;

mod lifecycle_apply;
use lifecycle_apply::*;

mod lifecycle_routes;
use lifecycle_routes::*;

mod live_delta;

mod live_render;

mod live_rows;
use live_rows::*;

mod live_turn;
use live_turn::*;

mod new_chat;
use new_chat::*;

mod owner;
use owner::*;

mod panels;
use panels::*;

mod pages;
mod pages_hub;

mod prompt_actions;
use prompt_actions::*;

mod row_pages;
use row_pages::*;

mod runtime;
use runtime::*;

mod setup;
use setup::*;

mod snapshot;
use snapshot::*;

mod terminal;
use terminal::*;
mod shells;
use shells::{shells_close, shells_create, shells_list};
mod branch;
use branch::*;

mod tool_output;
use tool_output::*;

mod transcript_routes;
use transcript_routes::*;

mod tree_routes;
use tree_routes::*;

mod ui;
use ui::*;

mod update;
use update::*;

mod usage;
use usage::*;

mod workspace_setup;
use workspace_setup::*;

pub use pages::pages;
pub use runtime::run;
pub use ui::{open_chat, ui, ui_state};

#[cfg(test)]
mod test_support;

#[cfg(test)]
mod test_metrics;

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;

    // This test re-runs itself by the exact path serve::tests::lifecycle_routes_child.
    include!("serve/tests/lifecycle_routes_child.rs");

    // This test re-runs itself by the exact path serve::tests::workspace_panel_routes_child.
    include!("serve/tests/workspace_panel_routes_child.rs");

    include!("../tests/serve_handoff/mod.rs");
}
