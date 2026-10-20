// Bounded provider-log reads and cumulative usage records.

/// The newest model and cumulative cost in a provider log; None means no source for that field.
#[derive(Debug, Default)]
pub struct UsageSnapshot {
    pub model: Option<String>,
    pub cost_usd: Option<f64>,
    /// A delta from claude_usage; a cumulative total from codex_usage_snapshot.
    pub tokens: Option<i64>,
}

pub const COUNTED_MESSAGES: usize = 64;
const CLAUDE_STOP_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
pub struct UsageState {
    pub logs: std::collections::BTreeMap<String, ClaudeLogUsage>,
}

#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct ClaudeLogUsage {
    pub offset: i64,
    pub tokens: i64,
    pub ids: std::collections::VecDeque<(String, i64)>,
}

pub struct ClaudeUsage {
    pub snapshot: UsageSnapshot,
    pub log: ClaudeLogUsage,
}

/// Read a bounded part of a Claude log, retaining each message's highest counted usage.
pub fn claude_usage(path: &std::path::Path, saved: &ClaudeLogUsage) -> Option<ClaudeUsage> {
    use std::io::{BufRead, Read, Seek, SeekFrom};
    let read = || -> Result<ClaudeUsage, Box<dyn std::error::Error>> {
        if !std::fs::metadata(path)?.is_file() {
            return Err("not a regular log file".into());
        }
        let mut file = std::fs::File::open(path)?;
        let len = file.metadata()?.len();
        let mut log = saved.clone();
        let mut offset = log.offset.max(0) as u64;
        if len < offset {
            offset = 0;
            log = ClaudeLogUsage::default();
        }
        // A prior Stop can end inside an oversized record. Check the preceding byte so its
        // remaining chunks are skipped, without storing a partial JSON record in usage_state.
        let mut continuation = false;
        let mut budget = CLAUDE_STOP_BYTES;
        if offset > 0 && offset < len {
            file.seek(SeekFrom::Start(offset - 1))?;
            let mut previous = [0];
            file.read_exact(&mut previous)?;
            continuation = previous[0] != b'\n';
            budget -= 1;
        }
        file.seek(SeekFrom::Start(offset))?;
        // Bound both the work and the allocation, even while the provider extends the log.
        budget = (len - offset).min(budget);
        let mut reader = std::io::BufReader::new(file.take(budget));
        let mut snapshot = UsageSnapshot::default();
        let mut line = Vec::new();
        while reader.read_until(b'\n', &mut line)? > 0 {
            if continuation {
                offset += line.len() as u64;
                continuation = !line.ends_with(b"\n");
                line.clear();
                continue;
            }
            let complete = line.ends_with(b"\n") || offset + line.len() as u64 == len;
            if !complete {
                if line.len() as u64 == budget {
                    // One record exceeds a Stop's budget. Skip it in bounded chunks.
                    offset += line.len() as u64;
                }
                break; // An ordinary cut record starts again at the next Stop.
            }
            let record = serde_json::from_slice::<serde_json::Value>(&line);
            if record.is_err() && !line.ends_with(b"\n") {
                break;
            }
            offset += line.len() as u64;
            if let Ok(record) = record {
                let mut latest = UsageSnapshot::default();
                read_usage_record("claude", &record, &mut latest);
                if latest.model.is_some() {
                    snapshot.model = latest.model;
                }
                if latest.cost_usd.is_some() {
                    snapshot.cost_usd = latest.cost_usd;
                }
                if record.get("type").and_then(serde_json::Value::as_str) == Some("assistant")
                    && record
                        .pointer("/message/model")
                        .and_then(serde_json::Value::as_str)
                        != Some("<synthetic>")
                    && let Some(id) = record
                        .pointer("/message/id")
                        .and_then(serde_json::Value::as_str)
                        .filter(|id| !id.is_empty())
                    && let Some(usage) = record
                        .pointer("/message/usage")
                        .and_then(serde_json::Value::as_object)
                    && let Some(tokens) = message_tokens(usage)
                {
                    let previous = log.ids.iter().position(|(seen, _)| seen == id);
                    let counted = previous.map_or(0, |index| log.ids[index].1);
                    let added = tokens.saturating_sub(counted).max(0);
                    if let Some(total) = log.tokens.checked_add(added)
                        && let Some(delta) = snapshot.tokens.unwrap_or(0).checked_add(added)
                    {
                        log.tokens = total;
                        snapshot.tokens = Some(delta);
                        if let Some(index) = previous {
                            log.ids[index].1 = counted.max(tokens);
                        } else {
                            log.ids.push_back((id.to_owned(), tokens));
                            if log.ids.len() > COUNTED_MESSAGES {
                                log.ids.pop_front();
                            }
                        }
                    }
                }
            }
            line.clear();
        }
        log.offset = i64::try_from(offset)?;
        if offset < len {
            // Model and cost from a partial read can be older than the launch values.
            snapshot.model = None;
            snapshot.cost_usd = None;
        }
        Ok(ClaudeUsage { snapshot, log })
    };
    read().ok()
}

fn message_tokens(usage: &serde_json::Map<String, serde_json::Value>) -> Option<i64> {
    [
        "input_tokens",
        "cache_creation_input_tokens",
        "cache_read_input_tokens",
        "output_tokens",
    ]
    .into_iter()
    .try_fold(0_i64, |total, key| {
        let count = usage.get(key).map_or(Some(0), serde_json::Value::as_i64)?;
        if count < 0 {
            return None;
        }
        total.checked_add(count)
    })
}

/// Read Codex cumulative usage from the last 1 MiB. An unreadable log yields no new values.
pub fn codex_usage_snapshot(path: &std::path::Path) -> UsageSnapshot {
    use std::io::{Read, Seek, SeekFrom};
    const TAIL_BYTES: u64 = 1024 * 1024;
    let read_tail = || -> std::io::Result<Vec<u8>> {
        // A stale log path may now name a directory or pipe; do not wait on a pipe's writer.
        if !std::fs::metadata(path)?.is_file() {
            return Err(std::io::Error::other("not a regular log file"));
        }
        let mut file = std::fs::File::open(path)?;
        let start = file.metadata()?.len().saturating_sub(TAIL_BYTES);
        file.seek(SeekFrom::Start(start))?;
        let mut bytes = Vec::new();
        file.take(TAIL_BYTES).read_to_end(&mut bytes)?;
        // The first line can start outside the tail; it is not a complete record.
        if start > 0 {
            let end = bytes
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(bytes.len(), |i| i + 1);
            bytes.drain(..end);
        }
        Ok(bytes)
    };
    read_tail()
        .map(|bytes| parse_usage_tail(&bytes))
        .unwrap_or_default()
}

/// Walk Codex records newest first; the first complete value of each kind wins.
fn parse_usage_tail(bytes: &[u8]) -> UsageSnapshot {
    let mut snapshot = UsageSnapshot::default();
    for line in bytes.rsplit(|byte| *byte == b'\n') {
        let Ok(record) = serde_json::from_slice::<serde_json::Value>(line) else {
            continue;
        };
        read_usage_record("codex", &record, &mut snapshot);
        if snapshot.tokens.is_some() && snapshot.model.is_some() {
            break;
        }
    }
    snapshot
}

fn read_usage_record(provider: &str, record: &serde_json::Value, snapshot: &mut UsageSnapshot) {
    let kind = record.get("type").and_then(serde_json::Value::as_str);
    match (provider, kind) {
        ("claude", Some("cost-state"))
            if snapshot.cost_usd.is_none()
                && record
                    .get("hasUnknownModelCost")
                    .and_then(serde_json::Value::as_bool)
                    == Some(false) =>
        {
            snapshot.cost_usd = record
                .get("totalCostUSD")
                .and_then(serde_json::Value::as_f64)
                .filter(|cost| cost.is_finite() && *cost >= 0.0);
        }
        ("claude", Some("assistant")) if snapshot.model.is_none() => {
            snapshot.model = real_model(
                record
                    .pointer("/message/model")
                    .and_then(serde_json::Value::as_str),
            );
        }
        ("codex", Some("turn_context")) if snapshot.model.is_none() => {
            snapshot.model = real_model(
                record
                    .pointer("/payload/model")
                    .and_then(serde_json::Value::as_str),
            );
        }
        ("codex", Some("event_msg"))
            if snapshot.tokens.is_none()
                && record
                    .pointer("/payload/type")
                    .and_then(serde_json::Value::as_str)
                    == Some("token_count") =>
        {
            snapshot.tokens = record
                .pointer("/payload/info/total_token_usage/total_tokens")
                .and_then(serde_json::Value::as_i64)
                .filter(|tokens| *tokens >= 0);
        }
        _ => {}
    }
}

fn real_model(value: Option<&str>) -> Option<String> {
    value
        .filter(|model| !model.is_empty() && *model != "<synthetic>")
        .map(str::to_string)
}
