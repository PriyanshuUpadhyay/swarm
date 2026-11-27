//! Weak worker registry; requests, followers and row slots own leases.
use super::*;
use swarm::view::transcript::worker_protocol::{
    Binding,
    v2::{Budget, Operation},
};

type Key = (String, PathBuf, String);
#[derive(Default)]
pub(super) struct Sources {
    held: tokio::sync::Mutex<HashMap<Key, Weak<Lease>>>,
    #[cfg(test)]
    pub(super) worker_cli: Option<PathBuf>,
}
pub(super) struct Lease {
    source: transcript_caller::Source,
    serial: tokio::sync::Mutex<()>,
}
pub(super) async fn acquire(app: &App, key: &str, log: &Log) -> Result<Arc<Lease>, ApiError> {
    let mut held = app.sources.held.lock().await;
    held.retain(|_, source| source.strong_count() > 0);
    let key = (key.to_owned(), log.path.clone(), log.format.clone());
    if let Some(source) = held.get(&key).and_then(Weak::upgrade)
        && !source.source.is_closed()
    {
        return Ok(source);
    }
    let mut command = tokio::process::Command::new(&app.cli);
    #[cfg(test)]
    if let Some(worker) = &app.sources.worker_cli {
        command = tokio::process::Command::new(worker);
    }
    command.args(["_transcript-worker", "--protocol", "2"]);
    let source = transcript_caller::Source::open(
        command,
        Binding {
            source_id: key.0.clone(),
            generation: 1,
        },
        &log.format,
        &log.path,
        Duration::from_secs(10),
    )
    .await
    .map_err(|_| unavailable())?;
    let lease = Arc::new(Lease {
        source,
        serial: tokio::sync::Mutex::new(()),
    });
    held.insert(key, Arc::downgrade(&lease));
    Ok(lease)
}
fn unavailable() -> ApiError {
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "transcript tool unavailable",
    )
}
fn invalid() -> ApiError {
    error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "invalid transcript output",
    )
}
impl Lease {
    pub(super) async fn read(&self, operation: Operation) -> Result<Value, ApiError> {
        self.read_in(operation, read_budget()?).await
    }

    async fn read_in(&self, operation: Operation, budget: Budget) -> Result<Value, ApiError> {
        let timeout = || error(StatusCode::GATEWAY_TIMEOUT, "transcript read timed out");
        let guard = budget
            .run(async { Ok(self.serial.lock().await) })
            .await
            .map_err(|_| timeout())?;
        let duration = Duration::from_millis(budget.remaining_ms().map_err(|_| timeout())?);
        let result = self.source.native(operation, duration).await.map_err(|_| {
            if budget.check().is_err() {
                timeout()
            } else {
                invalid()
            }
        });
        drop(guard);
        result
    }
}

fn read_budget() -> Result<Budget, ApiError> {
    Budget::new(Duration::from_secs(10)).map_err(|_| invalid())
}

type WindowLines = Vec<(u64, u64, Vec<Value>)>;

pub(super) fn lines(value: &Value) -> Result<WindowLines, ApiError> {
    value["lines"]
        .as_array()
        .ok_or_else(invalid)?
        .iter()
        .map(|line| {
            let start = line["start_byte"].as_u64().ok_or_else(invalid)?;
            let end = line["end_byte"]
                .as_u64()
                .filter(|end| *end >= start)
                .ok_or_else(invalid)?;
            let events = line["events"].as_array().ok_or_else(invalid)?.clone();
            Ok((start, end, events))
        })
        .collect()
}
async fn window_lines(
    source: &Lease,
    start: u64,
    end: u64,
    budget: Budget,
) -> Result<WindowLines, ApiError> {
    let payload = source
        .read_in(Operation::Window { start, end }, budget)
        .await?;
    if payload["start_byte"] != start || payload["end_byte"] != end {
        return Err(invalid());
    }
    lines(&payload)
}

pub(super) async fn window(
    app: &App,
    key: &str,
    log: &Log,
    start: u64,
    end: u64,
    seed_model: bool,
) -> Result<LiveRows, ApiError> {
    let source = acquire(app, key, log).await?;
    let budget = read_budget()?;
    let lines = window_lines(&source, start, end, budget).await?;
    let mut state = LiveRows::empty(start, log.format == "codex");
    if seed_model
        && lines
            .iter()
            .any(|(_, _, events)| events.iter().any(|event| view::model_fact(event).is_some()))
        && let Some(model) = model_before(&source, log, start, budget).await
    {
        state.seed_model(model);
    }
    for (start, end, events) in lines {
        state.ingest(start, end, &events);
    }
    state.source = Some(source);
    Ok(state)
}

/// A model outside the tail is evidence, not a row. Stop on an unread range rather than
/// guessing that an earlier fact stayed current through it.
async fn model_before(source: &Lease, log: &Log, boundary: u64, budget: Budget) -> Option<String> {
    let mut end = boundary;
    while end > 0 && budget.check().is_ok() {
        let (start, read_end) = row_offsets(&log.path, 200, Some(end)).ok()?;
        if read_end != end || start >= end || boundary - start > ROWS_FIND_LIMIT as u64 {
            return None;
        }
        let lines = window_lines(source, start, end, budget).await.ok()?;
        if let Some(model) = lines
            .iter()
            .rev()
            .flat_map(|(_, _, events)| events.iter().rev())
            .find_map(view::model_fact)
        {
            return Some(model.to_owned());
        }
        end = start;
    }
    None
}
