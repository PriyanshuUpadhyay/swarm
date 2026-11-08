//! A per-source authority keeps saved child failures apart from retained rows.
use std::{fs::File, os::fd::AsFd, path::Path, time::Duration};
use swarm::view::transcript::{
    reader_rows::ReaderRows,
    worker_protocol::{Binding, MAX_PART_BYTES, MAX_SOURCE_ID_BYTES, v2::*},
};
use tokio::sync::mpsc;

#[path = "transcript_worker_saved.rs"]
mod saved;

struct Held {
    file: File,
    binding: Binding,
    path: String,
    format: String,
    rows: Option<ReaderRows>,
}
struct Active {
    tag: Tag,
    budget: Budget,
    before: bool,
    running: saved::Running,
    terminal: Option<Terminal>,
}
enum Terminal {
    Cancel { id: u64, budget: Budget },
    Close { id: u64, budget: Budget },
}
struct Session {
    ready: bool,
    last: u64,
    transport: u64,
    held: Option<Held>,
    active: Option<Active>,
    events: mpsc::UnboundedSender<saved::Completion>,
    output: tokio::fs::File,
}

pub(super) fn run(args: &[String]) -> i32 {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(_) => return 1,
    };
    let result = runtime.block_on(async {
        match args {
            [flag, version] if flag == "--protocol" && version == "2" => serve().await,
            [flag, version, job_flag, job]
                if flag == "--protocol"
                    && version == "2"
                    && job_flag == "--saved-job"
                    && job.len() <= 16384 =>
            {
                saved::execute(job).await
            }
            _ => Err(Code::BadRequest),
        }
    });
    if result.is_ok() { 0 } else { 1 }
}

async fn serve() -> Result<()> {
    let mut input = tokio::net::unix::pipe::Receiver::from_owned_fd(
        std::io::stdin()
            .as_fd()
            .try_clone_to_owned()
            .map_err(|_| Code::Io)?,
    )
    .map_err(|_| Code::Io)?;
    let output = tokio::fs::File::from_std(File::from(
        std::io::stdout()
            .as_fd()
            .try_clone_to_owned()
            .map_err(|_| Code::Io)?,
    ));
    let (tx, mut controls) = mpsc::channel(8);
    let reader = tokio::spawn(async move {
        loop {
            let frame = read_frame(&mut input).await;
            let end = !matches!(frame, Ok(Some(_)));
            let received = tokio::time::Instant::now();
            if tx.send((received, frame)).await.is_err() || end {
                break;
            }
        }
    });
    let (events, mut completed) = mpsc::unbounded_channel();
    let mut session = Session {
        ready: false,
        last: 0,
        transport: 0,
        held: None,
        active: None,
        events,
        output,
    };
    let result = loop {
        tokio::select! {
            biased;
            control = controls.recv() => {
                let Some((received, frame)) = control else { break Err(Code::Io); };
                let bytes = match frame { Ok(Some(bytes)) => bytes, Ok(None) => break Ok(()), Err(code) => break Err(code) };
                let request: Request = match serde_json::from_slice(&bytes) { Ok(request) => request, Err(_) => break Err(Code::BadRequest) };
                let (id, binding, remaining_work_ms) = request.context();
                let binding = binding.cloned();
                let budget = received.checked_add(Duration::from_millis(remaining_work_ms)).map(Budget).ok_or(Code::BadRequest);
                let processed = match budget { Ok(budget) => session.control(request, budget).await, Err(code) => Err(code) };
                match processed {
                    Ok(true) => break Ok(()),
                    Ok(false) => {},
                    Err(code) => {
                        let budget = Budget::new(Duration::from_secs(1))?;
                        if let Err(code) = session.send(Reply::Fault { request_id: id, binding, code }, budget).await { break Err(code); }
                    }
                }
            }
            event = completed.recv(), if session.active.is_some() => {
                let Some(event) = event else { break Err(Code::Io); };
                match session.complete(event).await { Ok(true) => break Ok(()), Ok(false) => {}, Err(code) => break Err(code) }
            }
        }
    };
    // Retain child ownership until exit and both drains, including on a broken control pipe.
    if let Some(mut active) = session.active.take() {
        active.running.cancel(Budget::new(Duration::from_secs(1))?);
        let _ = active.running.task.await;
    }
    reader.abort();
    let _ = reader.await;
    result
}
impl Session {
    async fn send(&mut self, reply: Reply, budget: Budget) -> Result<()> {
        budget.run(write_frame(&mut self.output, &reply)).await
    }
    fn bound(&self, binding: &Binding) -> Result<()> {
        if self
            .held
            .as_ref()
            .is_some_and(|source| source.binding == *binding)
        {
            Ok(())
        } else {
            Err(Code::BindingMismatch)
        }
    }
    async fn control(&mut self, request: Request, budget: Budget) -> Result<bool> {
        budget.check()?;
        let (id, binding, _) = request.context();
        if let Some(id) = id {
            if !self.ready || id <= self.last {
                return Err(Code::BadRequest);
            }
            self.last = id;
        }
        if !matches!(request, Request::Open { .. })
            && let Some(binding) = binding
        {
            self.bound(binding)?;
        }
        match request {
            Request::Hello { protocol, .. } => {
                if self.ready {
                    return Err(Code::BadRequest);
                }
                if protocol != PROTOCOL {
                    return Err(Code::UnsupportedVersion);
                }
                self.send(
                    Reply::Ready {
                        protocol: PROTOCOL,
                        formats: vec!["claude".into()],
                    },
                    budget,
                )
                .await?;
                self.ready = true;
            }
            Request::Open {
                request_id,
                binding,
                format,
                path,
                ..
            } => {
                if self.held.is_some()
                    || binding.source_id.is_empty()
                    || binding.source_id.len() > MAX_SOURCE_ID_BYTES
                    || format != "claude"
                    || !Path::new(&path).is_absolute()
                {
                    return Err(Code::BadRequest);
                }
                if !Path::new(&path).metadata().map_err(|_| Code::Io)?.is_file() {
                    return Err(Code::BadRequest);
                }
                budget.check()?;
                let file = File::open(&path).map_err(|_| Code::Io)?;
                if !file.metadata().map_err(|_| Code::Io)?.is_file() {
                    return Err(Code::BadRequest);
                }
                budget.check()?;
                self.held = Some(Held {
                    file,
                    binding: binding.clone(),
                    path,
                    format,
                    rows: None,
                });
                self.send(
                    Reply::Opened {
                        request_id,
                        binding,
                    },
                    budget,
                )
                .await?;
            }
            Request::Page {
                request_id,
                binding,
                tail,
                before,
                ..
            } => {
                if self.active.is_some() || !(1..=2000).contains(&tail) {
                    return Err(Code::BadRequest);
                }
                let source = self.held.as_ref().ok_or(Code::BadRequest)?;
                let (start_byte, end_byte) = saved::range(&source.file, tail, before, budget)?;
                self.transport = self.transport.checked_add(1).ok_or(Code::Core)?;
                let tag = Tag {
                    request_id,
                    binding,
                    transport_id: self.transport,
                };
                let job = saved::Job {
                    tag: tag.clone(),
                    path: source.path.clone(),
                    format: source.format.clone(),
                    start_byte,
                    end_byte,
                    remaining_work_ms: budget.remaining_ms()?,
                };
                let running = saved::spawn(
                    source.file.try_clone().map_err(|_| Code::Io)?,
                    job,
                    budget,
                    self.events.clone(),
                )?;
                self.active = Some(Active {
                    tag: tag.clone(),
                    budget,
                    before: before.is_some(),
                    running,
                    terminal: None,
                });
                self.send(Reply::Begin { tag }, budget).await?;
            }
            Request::Cancel {
                request_id,
                target_request_id,
                ..
            } => {
                let active = self.active.as_mut().ok_or(Code::BadRequest)?;
                if target_request_id != active.tag.request_id || active.terminal.is_some() {
                    return Err(Code::BadRequest);
                }
                active.terminal = Some(Terminal::Cancel {
                    id: request_id,
                    budget,
                });
                active.running.cancel(budget);
            }
            Request::Close {
                request_id,
                binding,
                ..
            } => {
                if let Some(active) = self.active.as_mut() {
                    if active.terminal.is_some() {
                        return Err(Code::BadRequest);
                    }
                    active.terminal = Some(Terminal::Close {
                        id: request_id,
                        budget,
                    });
                    active.running.cancel(budget);
                } else {
                    self.held.take();
                    self.send(
                        Reply::Closed {
                            request_id,
                            binding,
                        },
                        budget,
                    )
                    .await?;
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }
    async fn complete(&mut self, completion: saved::Completion) -> Result<bool> {
        let active = self.active.take().ok_or(Code::Core)?;
        if completion.tag != active.tag {
            return Err(Code::BindingMismatch);
        }
        active.running.task.await.map_err(|_| Code::Io)?;
        if let Some(terminal) = active.terminal {
            return match terminal {
                Terminal::Cancel { id, budget } => {
                    self.send(
                        Reply::Fenced {
                            tag: active.tag.clone(),
                        },
                        budget,
                    )
                    .await?;
                    self.send(
                        Reply::Cancelled {
                            request_id: id,
                            binding: active.tag.binding,
                            target_request_id: active.tag.request_id,
                            transport_id: active.tag.transport_id,
                            exit: completion.exit,
                        },
                        budget,
                    )
                    .await?;
                    Ok(false)
                }
                Terminal::Close { id, budget } => {
                    self.held.take();
                    self.send(
                        Reply::Closed {
                            request_id: id,
                            binding: active.tag.binding,
                        },
                        budget,
                    )
                    .await?;
                    Ok(true)
                }
            };
        }
        let result = completion.result.and_then(|payload| {
            active.budget.check()?;
            self.page(payload, active.before, active.budget)
        });
        let bytes = match result {
            Ok(bytes) => bytes,
            Err(code) => {
                self.send(
                    Reply::Fault {
                        request_id: Some(active.tag.request_id),
                        binding: Some(active.tag.binding),
                        code,
                    },
                    Budget::new(Duration::from_secs(1))?,
                )
                .await?;
                return Ok(false);
            }
        };
        let mut part_count = 0;
        for bytes in bytes.chunks(MAX_PART_BYTES) {
            self.send(
                Reply::Part {
                    tag: active.tag.clone(),
                    part_index: part_count,
                    bytes: bytes.into(),
                },
                active.budget,
            )
            .await?;
            part_count += 1;
        }
        self.send(
            Reply::Commit {
                tag: active.tag,
                part_count,
                payload_bytes: bytes.len() as u64,
            },
            active.budget,
        )
        .await?;
        Ok(false)
    }
    fn page(&mut self, payload: saved::Payload, before: bool, budget: Budget) -> Result<Vec<u8>> {
        let held = self.held.as_mut().ok_or(Code::Core)?;
        let mut page = ReaderRows::empty(payload.start_byte, false);
        for line in payload.lines {
            budget.check()?;
            page.ingest(line.start_byte, line.end_byte, &line.events);
        }
        let mut state = held.rows.clone().unwrap_or_else(|| page.clone());
        let page = if !before {
            state.cached_tail(payload.start_byte, payload.end_byte)
        } else {
            None
        }
        .unwrap_or_else(|| state.merge_page(page, before));
        let bytes = serde_json::to_vec(&serde_json::json!({
            "page": {"start_offset": payload.start_byte, "end_offset": payload.end_byte},
            "rows": page.rows, "usage": page.usage, "remove": page.remove,
        }))
        .map_err(|_| Code::Core)?;
        budget.check()?;
        held.rows = Some(state);
        Ok(bytes)
    }
}
