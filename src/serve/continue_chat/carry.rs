//! Context carried to a linked chat.

use super::*;

pub(super) async fn prepare_carry(
    app: &App,
    body: &ContinueChat,
    call: &CliCall,
    summary_wait: Duration,
    poll: Duration,
) -> Result<Option<String>, ApiError> {
    let old = &body.old_session;
    continue_phase(app, old, "preparing")?;
    let chair = chair_agent(app, call).await?;
    let alive = chair["alive"] == true;

    if alive {
        require_idle(app, old, &chair).await?;
    }

    match body.carry {
        Carry::Clean => Ok(None),
        Carry::File => {
            let chain = read_connection(&app.root)
                .and_then(|db| swarm::store::continuation_logs(&db, old))
                .map_err(actions::history_read_error)?;

            let mut text = Vec::new();
            for row in chain {
                let log = match resolve_link_log(&row) {
                    Ok(log) => log,
                    Err((StatusCode::NOT_FOUND, _)) if stored_link_path(&row).is_none() => {
                        continue;
                    }
                    Err((StatusCode::NOT_FOUND, body)) => return Err((StatusCode::CONFLICT, body)),
                    Err(err) => return Err(err),
                };

                if let Some(part) = handoff::chat_text(&read_handoff_log(app, &row.id, &log).await?)
                {
                    text.push(part);
                }
            }

            if text.is_empty() {
                return Err(error(
                    StatusCode::CONFLICT,
                    "The current chat has no messages to carry forward",
                ));
            }

            let path = chat_file(app, old, text.join("\n\n")).await?;
            Ok(Some(handoff::file_message(&path.to_string_lossy())))
        }
        Carry::Summary => {
            if !alive {
                return Err(error(
                    StatusCode::CONFLICT,
                    "The old agent is not running to write a summary",
                ));
            }

            let baseline = handoff_rows(app, old).await?.len();
            continue_phase(app, old, "summarizing")?;
            let deadline = tokio::time::Instant::now() + summary_wait;
            let context = tokio::time::timeout_at(deadline, async {
                run_cli(
                    app,
                    call.args(&["type", "orchestrator"]),
                    Some(handoff::REQUEST.into()),
                )
                .await?;

                loop {
                    tokio::time::sleep(poll).await;
                    if let Ok(rows) = handoff_rows(app, old).await
                        && let Some(summary) = handoff::summary_turn(&rows, baseline)
                    {
                        return summary.ok_or_else(|| {
                            error(
                                StatusCode::CONFLICT,
                                "The old agent did not write a summary",
                            )
                        });
                    }
                }
            })
            .await
            .map_err(|_| {
                error(
                    StatusCode::GATEWAY_TIMEOUT,
                    "Timed out waiting for a chat summary",
                )
            })??;

            Ok(Some(handoff::summary_message(&context)))
        }
        Carry::Resume => unreachable!("native resume does not carry chat context"),
    }
}
