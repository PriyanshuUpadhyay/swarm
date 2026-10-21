//! Server actions.

use super::*;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ActionTarget {
    pub(super) session: String,
    pub(super) agent: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SendAction {
    pub(super) session: String,
    pub(super) agent: String,
    pub(super) text: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct KeyAction {
    pub(super) session: String,
    pub(super) agent: String,
    pub(super) key: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AnswerAction {
    pub(super) session: String,
    pub(super) agent: String,
    pub(super) prompt_id: String,
    pub(super) choice: usize,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SessionAction {
    pub(super) ids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NewChat {
    pub(super) cwd: String,
    pub(super) role: Option<String>,
    pub(super) provider: Option<String>,
    pub(super) model: Option<String>,
    pub(super) account: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ResumeChat {
    pub(super) session: String,
}

#[derive(Deserialize)]
pub(super) enum Action {
    Send(SendAction),
    PullBack(ActionTarget),
    Interrupt(ActionTarget),
    Key(KeyAction),
    Answer(AnswerAction),
    Close(ActionTarget),
    Archive(SessionAction),
    Unarchive(SessionAction),
    New(NewChat),
    Resume(ResumeChat),
    Continue(ContinueChat),
    Archived,
    Choices(choices::ChoiceWrite),
    RefreshView,
    Lifecycle(LifecycleAction),
    UsageRefresh(UsageRefresh),
    Hud(HudWrite),
    Setup(SetupWrite),
    Owner(OwnerWrite),
    Import(ImportWrite),
}

pub(super) struct ActionJob {
    pub(super) action: Action,
    pub(super) reply: oneshot::Sender<Result<Value, ApiError>>,
}

pub(super) struct CliCall {
    pub(super) args: Vec<String>,
    pub(super) cwd: PathBuf,
    pub(super) session: Option<String>,
    pub(super) adapter: String,
}

impl CliCall {
    pub(super) fn args(&self, args: &[&str]) -> Self {
        Self {
            args: args.iter().map(|arg| (*arg).to_owned()).collect(),
            cwd: self.cwd.clone(),
            session: self.session.clone(),
            adapter: self.adapter.clone(),
        }
    }
}

pub(super) async fn chat_action(
    State(app): State<App>,
    uri: Uri,
    body: Result<Json<Value>, JsonRejection>,
) -> Result<Json<Value>, ApiError> {
    let Json(body) = body.map_err(|rejection| {
        let status = if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
            StatusCode::PAYLOAD_TOO_LARGE
        } else {
            StatusCode::BAD_REQUEST
        };
        error(status, &rejection.body_text())
    })?;
    if !body.is_object() {
        return Err(error(StatusCode::BAD_REQUEST, "JSON object required"));
    }
    let kind = match uri.path() {
        "/api/chat/send" => "Send",
        "/api/chat/pull-back" => "PullBack",
        "/api/chat/interrupt" => "Interrupt",
        "/api/chat/key" => "Key",
        "/api/chat/answer" => "Answer",
        "/api/agent/close" => "Close",
        "/api/sessions/archive" => "Archive",
        "/api/sessions/unarchive" => "Unarchive",
        "/api/chat/new" => "New",
        "/api/chat/resume" => "Resume",
        _ => return Err(error(StatusCode::NOT_FOUND, "unknown action")),
    };
    let action = serde_json::from_value(json!({kind: body}))
        .map_err(|err| error(StatusCode::BAD_REQUEST, &err.to_string()))?;
    enqueue_action(&app, action).await.map(Json)
}

pub(super) async fn archived_sessions(State(app): State<App>) -> Result<Json<Value>, ApiError> {
    enqueue_action(&app, Action::Archived).await.map(Json)
}

pub(super) async fn enqueue_action(app: &App, action: Action) -> Result<Value, ApiError> {
    let (reply, result) = oneshot::channel();
    app.actions
        .send(ActionJob { action, reply })
        .await
        .map_err(|_| {
            error(
                StatusCode::SERVICE_UNAVAILABLE,
                "action coordinator stopped",
            )
        })?;
    result.await.map_err(|_| {
        error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "action coordinator failed",
        )
    })?
}

impl Action {
    pub(super) fn lane(&self) -> Option<&str> {
        match self {
            Self::Send(body) => Some(&body.session),
            Self::Continue(body) => Some(&body.old_session),
            Self::Resume(body) => Some(&body.session),
            Self::Interrupt(body) | Self::Close(body) | Self::PullBack(body) => Some(&body.session),
            Self::Key(body) => Some(&body.session),
            Self::Answer(body) => Some(&body.session),
            Self::New(_)
            | Self::Archive(_)
            | Self::Unarchive(_)
            | Self::Archived
            | Self::Choices(_)
            | Self::Lifecycle(_)
            | Self::UsageRefresh(_)
            | Self::Hud(_)
            | Self::Owner(_)
            | Self::Import(_)
            | Self::RefreshView => None,
            // A plugin install can take minutes, so it waits in its own lane, not in the one that
            // folds and archives share.
            Self::Setup(_) => Some("setup"),
        }
    }
}

pub(super) fn start_action_queue(app: &App, mut queue: mpsc::Receiver<ActionJob>) {
    let weak = Arc::downgrade(app);
    tokio::spawn(async move {
        let mut pending: VecDeque<(Option<String>, ActionJob)> = VecDeque::new();
        let mut active = HashSet::new();
        let mut workers = tokio::task::JoinSet::new();
        let mut accepting = true;
        loop {
            let mut index = 0;
            while index < pending.len() {
                if active.contains(&pending[index].0) {
                    index += 1;
                    continue;
                }
                let (lane, job) = pending.remove(index).unwrap();
                let Some(app) = weak.upgrade() else { return };
                active.insert(lane.clone());
                workers.spawn(async move {
                    // An accepted action continues if its HTTP connection closes.
                    let changed = !matches!(
                        job.action,
                        Action::Archived
                            | Action::RefreshView
                            | Action::Choices(_)
                            | Action::Lifecycle(_)
                            | Action::UsageRefresh(_)
                            | Action::Hud(_)
                            | Action::Setup(_)
                            | Action::Owner(_)
                    );
                    let result = execute_action(&app, job.action).await;
                    if changed {
                        app.hub.lock().unwrap().bump();
                    }
                    let _ = job.reply.send(result);
                    lane
                });
            }
            if !accepting && workers.is_empty() {
                break;
            }
            tokio::select! {
                // Bound queued and running work; keep only lanes with active jobs in memory.
                job = queue.recv(), if accepting && pending.len() + active.len() < 64 => {
                    match job {
                        Some(job) => pending.push_back((job.action.lane().map(str::to_owned), job)),
                        None => accepting = false,
                    }
                }
                result = workers.join_next(), if !workers.is_empty() => {
                    match result {
                        Some(Ok(lane)) => { active.remove(&lane); }
                        // A failed owner stops all lanes rather than retrying uncertain mutations.
                        _ => break,
                    }
                }
            }
        }
    });
}

pub(super) fn action_session(app: &Server, session: &str) -> Result<CliCall, ApiError> {
    let connection = read_connection(&app.root)
        .map_err(|err| error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()))?;
    let row = connection
        .query_row(
            "SELECT cwd, adapter FROM session WHERE id = ?1",
            [session],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
        )
        .optional()
        .map_err(|err| error(StatusCode::INTERNAL_SERVER_ERROR, &err.to_string()))?;
    let (cwd, adapter) = row.ok_or_else(|| error(StatusCode::NOT_FOUND, "unknown session"))?;
    Ok(CliCall {
        args: Vec::new(),
        cwd: cwd.into(),
        session: Some(session.to_owned()),
        adapter: adapter.unwrap_or_default(),
    })
}

#[cfg(test)]
#[path = "actions/tests.rs"]
mod tests;
