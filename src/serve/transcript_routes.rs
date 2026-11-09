//! Server transcript routes.

use super::*;

#[derive(Deserialize)]
pub(super) struct TranscriptQuery {
    pub(super) session: String,
    pub(super) agent: String,
    pub(super) tail: u16,
    pub(super) before: Option<u64>,
}

pub(super) struct Log {
    pub(super) path: PathBuf,
    pub(super) format: String,
}

pub(super) fn resolve_log(root: &Path, session: &str, agent: &str) -> Result<Log, ApiError> {
    let connection = read_connection(root)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "database read failed"))?;
    let row = connection
        .query_row(
            "SELECT chair_log, chair_provider, chair_id FROM session WHERE id = ?1",
            [session],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            },
        )
        .optional()
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "session read failed"))?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "unknown session"))?;
    let (path, format, pi_id) = if agent == "chair" {
        row
    } else {
        let row = swarm::store::agents(&connection, session)
            .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "agent read failed"))?
            .into_iter()
            .find(|row| row.id == agent)
            .ok_or_else(|| error(StatusCode::NOT_FOUND, "unknown agent"))?;
        (row.log, row.provider, Some(format!("{session}-{agent}")))
    };
    // A chair's log is found by its id as the snapshot finds it. pi has no hook to report a seat's
    // log; `launch` gave it the id `<session>-<agent>` (ADR 0086).
    // ponytail: each read scans the session folders until a log is stored; store the found path
    // if a read gets slow.
    let path = path.filter(|path| !path.is_empty()).or_else(|| {
        if agent == "chair"
            && let Some(log) = swarm::store::sessions_with_archived(&connection, false)
                .ok()
                .and_then(|rows| rows.into_iter().find(|row| row.id == session))
                .and_then(|row| chair_log(&row, false))
        {
            return Some(log);
        }
        (format.as_deref() == Some("pi"))
            .then(|| crate::pi_chair_log(pi_id.as_deref()?))
            .flatten()
            .map(|path| path.to_string_lossy().into_owned())
    });
    let path = path.ok_or_else(|| error(StatusCode::NOT_FOUND, "no transcript log"))?;
    let format = format
        .filter(|format| swarm::view::transcript::LOG_FORMATS.contains(&format.as_str()))
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "no transcript format"))?;
    if !Path::new(&path).is_file() {
        return Err(error(StatusCode::NOT_FOUND, "transcript log unavailable"));
    }
    Ok(Log {
        path: path.into(),
        format,
    })
}

pub(super) async fn transcript(
    State(app): State<App>,
    Query(query): Query<TranscriptQuery>,
) -> Result<Json<Value>, ApiError> {
    if !(1..=2000).contains(&query.tail) {
        return Err(error(StatusCode::BAD_REQUEST, "tail must be 1..2000"));
    }
    let log = resolve_log(&app.root, &query.session, &query.agent)?;
    let source =
        source_backend::acquire(&app, &format!("{}:{}", query.session, query.agent), &log).await?;
    use swarm::view::transcript::worker_protocol::v2::Operation;
    let payload = source
        .read(Operation::Raw {
            tail: query.tail,
            before: query.before,
        })
        .await?;
    let events: Vec<_> = source_backend::lines(&payload)?
        .into_iter()
        .flat_map(|(_, _, events)| events)
        .collect();
    Ok(Json(
        json!({"page": {"start_offset": payload["start_byte"], "end_offset": payload["end_byte"]}, "events": events}),
    ))
}
