//! One executor receives the held descriptor on stdin and uses positional reads.
use super::*;
use serde::{Deserialize, Serialize};
use std::{os::unix::fs::FileExt, process::Stdio};
use swarm::view::transcript::source::{EndPolicy, NativeFormat, SnapshotReader, SourceMeta};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
    sync::oneshot,
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Job {
    pub tag: Tag,
    pub path: String,
    pub format: String,
    pub start_byte: u64,
    pub end_byte: u64,
    pub remaining_work_ms: u64,
    #[serde(default)]
    pub eof: bool,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Payload {
    pub start_byte: u64,
    pub end_byte: u64,
    pub lines: Vec<Line>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Line {
    pub start_byte: u64,
    pub end_byte: u64,
    pub events: Vec<serde_json::Value>,
}
pub(super) struct Completion {
    pub tag: Tag,
    pub result: Result<Payload>,
    pub exit: Exit,
}
pub(super) struct Running {
    cancel: Option<oneshot::Sender<Budget>>,
    pub task: tokio::task::JoinHandle<()>,
}
impl Running {
    pub fn cancel(&mut self, budget: Budget) {
        if let Some(cancel) = self.cancel.take() {
            let _ = cancel.send(budget);
        }
    }
}
pub(super) fn spawn(
    file: File,
    mut job: Job,
    budget: Budget,
    completed: mpsc::UnboundedSender<Completion>,
) -> Result<Running> {
    job.remaining_work_ms = budget.remaining_ms()?;
    let text = serde_json::to_string(&job).map_err(|_| Code::Core)?;
    if text.len() > 16384 {
        return Err(Code::BadRequest);
    }
    let mut command = Command::new(std::env::current_exe().map_err(|_| Code::Io)?);
    command
        .args([
            "_transcript-worker",
            "--protocol",
            "2",
            "--saved-job",
            &text,
        ])
        .stdin(Stdio::from(file))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let mut child = command.spawn().map_err(|_| Code::Io)?;
    let mut stdout = child.stdout.take().ok_or(Code::Io)?;
    let stderr = child.stderr.take().ok_or(Code::Io)?;
    let tag = job.tag.clone();
    let output_tag = tag.clone();
    let output = tokio::spawn(async move {
        let result = read_payload(&mut stdout, &output_tag).await;
        let drained = drain(stdout).await;
        result.and_then(|payload| drained.map(|()| payload))
    });
    let stderr = tokio::spawn(drain(stderr));
    let (cancel, mut cancelled) = oneshot::channel();
    let task = tokio::spawn(async move {
        let mut completion_budget = budget;
        let (status, fault) = tokio::select! {
            biased;
            cancelled = &mut cancelled => {
                completion_budget = cancelled.unwrap_or_else(|_| Budget(tokio::time::Instant::now()));
                let _ = child.start_kill();
                (child.wait().await, Some(Code::Cancelled))
            },
            _ = tokio::time::sleep_until(budget.0) => {
                let _ = child.start_kill();
                (child.wait().await, Some(Code::Deadline))
            },
            status = child.wait() => (status, None),
        };
        let (output, stderr) = tokio::join!(output, stderr);
        let mut result = output.map_err(|_| Code::Io).and_then(|r| r);
        if let Err(code) = stderr.map_err(|_| Code::Io).and_then(|r| r) {
            result = Err(code);
        }
        let exit = match status {
            Ok(status) => {
                use std::os::unix::process::ExitStatusExt;
                if !status.success() {
                    result = Err(Code::Core);
                }
                match status.code() {
                    Some(code) => Exit::Code { code },
                    None => Exit::Signal {
                        signal: status.signal().unwrap_or(0),
                    },
                }
            }
            Err(_) => {
                result = Err(Code::Io);
                Exit::Code { code: 1 }
            }
        };
        if let Some(fault) = fault {
            result = Err(fault);
        }
        if completion_budget.check().is_err() {
            result = Err(Code::Deadline);
        }
        let _ = completed.send(Completion { tag, result, exit });
    });
    Ok(Running {
        cancel: Some(cancel),
        task,
    })
}
async fn read_payload(input: &mut (impl AsyncRead + Unpin), tag: &Tag) -> Result<Payload> {
    let mut begun = false;
    let mut committed = false;
    let mut bytes = Vec::new();
    let mut parts = 0;
    while let Some(frame) = read_frame(input).await? {
        let reply: Reply = serde_json::from_slice(&frame).map_err(|_| Code::BadFrame)?;
        match reply {
            Reply::Begin { tag: actual } if !begun && actual == *tag => begun = true,
            Reply::Part {
                tag: actual,
                part_index,
                bytes: part,
            } if begun
                && !committed
                && actual == *tag
                && part_index == parts
                && !part.is_empty()
                && part.len() <= MAX_PART_BYTES =>
            {
                parts += 1;
                bytes.extend(part);
            }
            Reply::Commit {
                tag: actual,
                part_count,
                payload_bytes,
            } if begun
                && !committed
                && actual == *tag
                && part_count == parts
                && payload_bytes == bytes.len() as u64 =>
            {
                committed = true
            }
            _ => return Err(Code::BadFrame),
        }
    }
    if !committed {
        return Err(Code::Core);
    }
    serde_json::from_slice(&bytes).map_err(|_| Code::Core)
}
async fn drain(mut input: impl AsyncRead + Unpin) -> Result<()> {
    let mut buffer = [0; MAX_PART_BYTES];
    while input.read(&mut buffer).await.map_err(|_| Code::Io)? != 0 {}
    Ok(())
}

pub(super) fn range(
    file: &File,
    tail: u16,
    before: Option<u64>,
    budget: Budget,
) -> Result<(u64, u64)> {
    range_inner(file, tail, before, budget, true)
}
pub(super) fn raw_range(
    file: &File,
    tail: u16,
    before: Option<u64>,
    budget: Budget,
) -> Result<(u64, u64)> {
    range_inner(file, tail, before, budget, false)
}
fn range_inner(
    file: &File,
    tail: u16,
    before: Option<u64>,
    budget: Budget,
    complete: bool,
) -> Result<(u64, u64)> {
    let mut end = file
        .metadata()
        .map_err(|_| Code::Io)?
        .len()
        .min(before.unwrap_or(u64::MAX));
    let mut buffer = [0; MAX_PART_BYTES];
    let mut cursor = end;
    while complete && cursor > 0 {
        budget.check()?;
        let count = cursor.min(buffer.len() as u64) as usize;
        let start = cursor - count as u64;
        file.read_exact_at(&mut buffer[..count], start)
            .map_err(|_| Code::Io)?;
        if let Some(index) = buffer[..count].iter().rposition(|byte| *byte == b'\n') {
            end = start + index as u64 + 1;
            break;
        }
        cursor = start;
    }
    if complete && cursor == 0 {
        end = 0;
    }
    let (mut cursor, mut lines) = (end, 0);
    while cursor > 0 {
        budget.check()?;
        let count = cursor.min(buffer.len() as u64) as usize;
        let start = cursor - count as u64;
        file.read_exact_at(&mut buffer[..count], start)
            .map_err(|_| Code::Io)?;
        for index in (0..count).rev() {
            let offset = start + index as u64;
            if buffer[index] == b'\n' && offset + 1 != end {
                lines += 1;
                if lines == tail {
                    return Ok((offset + 1, end));
                }
            }
        }
        cursor = start;
    }
    Ok((0, end))
}

pub(super) fn line_end(file: &File, start: u64, budget: Budget) -> Result<u64> {
    let length = file.metadata().map_err(|_| Code::Io)?.len();
    if start >= length {
        return Err(Code::BadRequest);
    }
    let mut offset = start;
    let mut buffer = [0; MAX_PART_BYTES];
    while offset < length {
        budget.check()?;
        let count = (length - offset).min(buffer.len() as u64) as usize;
        file.read_exact_at(&mut buffer[..count], offset)
            .map_err(|_| Code::Io)?;
        if let Some(i) = buffer[..count].iter().position(|b| *b == b'\n') {
            return Ok(offset + i as u64 + 1);
        }
        offset += count as u64;
    }
    Ok(length)
}

pub(super) async fn execute(text: &str) -> Result<()> {
    let job: Job = serde_json::from_str(text).map_err(|_| Code::BadRequest)?;
    let mut budget = Budget::new(Duration::from_millis(job.remaining_work_ms))?;
    let format = match job.format.as_str() {
        "claude" => NativeFormat::Claude,
        "codex" => NativeFormat::Codex,
        "agy" => NativeFormat::Agy,
        _ => return Err(Code::BadRequest),
    };
    let file = File::from(
        std::io::stdin()
            .as_fd()
            .try_clone_to_owned()
            .map_err(|_| Code::Io)?,
    );
    let meta = file.metadata().map_err(|_| Code::Io)?;
    if !meta.is_file() || job.start_byte > job.end_byte || job.end_byte > meta.len() {
        return Err(Code::BadRequest);
    }
    for offset in [job.start_byte, job.end_byte] {
        if offset != 0 && (offset == job.start_byte || !job.eof) {
            let mut byte = [0];
            file.read_exact_at(&mut byte, offset - 1)
                .map_err(|_| Code::Io)?;
            if byte != [b'\n'] {
                return Err(Code::BadRequest);
            }
        }
    }
    let mut reader = SnapshotReader::with_open_path(
        SourceMeta {
            source_id: job.tag.binding.source_id.clone(),
        },
        job.start_byte,
        if job.eof {
            EndPolicy::SnapshotEof
        } else {
            EndPolicy::CompleteLines
        },
        format,
        &job.path,
    );
    let mut offset = job.start_byte;
    let mut buffer = [0; MAX_PART_BYTES];
    while offset < job.end_byte {
        budget.check()?;
        let count = (job.end_byte - offset).min(buffer.len() as u64) as usize;
        file.read_exact_at(&mut buffer[..count], offset)
            .map_err(|_| Code::Io)?;
        reader
            .push(&buffer[..count], &mut budget)
            .map_err(|_| Code::Core)?;
        offset += count as u64;
    }
    let native = reader.finish_native(&mut budget).map_err(|_| Code::Core)?;
    let mut lines = Vec::new();
    for line in native.lines {
        budget.check()?;
        let events = line
            .events
            .iter()
            .map(|event| serde_json::to_value(event).map_err(|_| Code::Core))
            .collect::<Result<Vec<_>>>()?;
        lines.push(Line {
            start_byte: line.start_byte,
            end_byte: line.end_byte,
            events,
        });
    }
    let bytes = serde_json::to_vec(&Payload {
        start_byte: job.start_byte,
        end_byte: job.end_byte,
        lines,
    })
    .map_err(|_| Code::Core)?;
    let mut output = tokio::fs::File::from_std(File::from(
        std::io::stdout()
            .as_fd()
            .try_clone_to_owned()
            .map_err(|_| Code::Io)?,
    ));
    budget
        .run(write_frame(
            &mut output,
            &Reply::Begin {
                tag: job.tag.clone(),
            },
        ))
        .await?;
    let mut part_count = 0;
    for bytes in bytes.chunks(MAX_PART_BYTES) {
        budget
            .run(write_frame(
                &mut output,
                &Reply::Part {
                    tag: job.tag.clone(),
                    part_index: part_count,
                    bytes: bytes.into(),
                },
            ))
            .await?;
        part_count += 1;
    }
    budget
        .run(write_frame(
            &mut output,
            &Reply::Commit {
                tag: job.tag,
                part_count,
                payload_bytes: bytes.len() as u64,
            },
        ))
        .await
}
