//! Runs the private snapshot worker over framed standard input and output.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::time::{Duration, Instant};
use swarm::view::transcript::source::{EndPolicy, SnapshotError, SnapshotReader, SourceMeta, Work};
use swarm::view::transcript::worker_protocol::{
    Binding, FaultCode, MAX_FRAME_BYTES, MAX_PART_BYTES, MAX_SOURCE_ID_BYTES, PROTOCOL, Reply,
    Request,
};

struct Budget {
    deadline: Instant,
}

impl Budget {
    fn new(received: Instant, remaining_work_ms: u64) -> Result<Self, FaultCode> {
        if remaining_work_ms == 0 {
            return Err(FaultCode::BadRequest);
        }
        let deadline = received
            .checked_add(Duration::from_millis(remaining_work_ms))
            .ok_or(FaultCode::BadRequest)?;
        Ok(Self { deadline })
    }

    fn check_work(&mut self) -> Result<(), FaultCode> {
        self.check().map_err(core_fault)
    }
}

impl Work for Budget {
    fn check(&mut self) -> Result<(), SnapshotError> {
        if Instant::now() >= self.deadline {
            Err(SnapshotError::Deadline)
        } else {
            Ok(())
        }
    }
}

struct Source {
    file: File,
    binding: Binding,
}

#[derive(Default)]
struct Session {
    ready: bool,
    source: Option<Source>,
    last_request_id: Option<u64>,
}

struct Failure {
    request_id: Option<u64>,
    binding: Option<Binding>,
    code: FaultCode,
}

impl Failure {
    fn bare(code: FaultCode) -> Self {
        Self {
            request_id: None,
            binding: None,
            code,
        }
    }
}

pub(crate) fn run(args: &[String]) -> i32 {
    let input = io::stdin();
    let output = io::stdout();
    let mut input = input.lock();
    let mut output = output.lock();
    let result = if args.len() == 2 && args[0] == "--protocol" && args[1] == "1" {
        serve(&mut input, &mut output)
    } else {
        Err(Failure::bare(FaultCode::BadRequest))
    };
    match result {
        Ok(()) => 0,
        Err(failure) => {
            let status = match failure.code {
                FaultCode::BadFrame
                | FaultCode::BadRequest
                | FaultCode::UnsupportedVersion
                | FaultCode::UnsupportedFormat
                | FaultCode::BindingMismatch
                | FaultCode::InvalidRange => 2,
                _ => 1,
            };
            // Fault reporting has no expired work budget, and exposes no source diagnostics.
            let reply = Reply::Fault {
                request_id: failure.request_id,
                binding: failure.binding,
                code: failure.code,
            };
            if let Ok(bytes) = serde_json::to_vec(&reply) {
                let _ = write_frame(&mut output, &bytes);
            }
            status
        }
    }
}

fn serve(input: &mut impl Read, output: &mut impl Write) -> Result<(), Failure> {
    let mut session = Session::default();
    loop {
        let Some(bytes) = read_frame(input).map_err(Failure::bare)? else {
            return Ok(());
        };
        // Waiting for the next complete request is idle, outside its active work budget.
        let received = Instant::now();
        let request: Request = serde_json::from_slice(&bytes).map_err(|error| {
            Failure::bare(if error.is_syntax() || error.is_eof() {
                FaultCode::BadFrame
            } else {
                FaultCode::BadRequest
            })
        })?;
        let (request_id, binding) = match &request {
            Request::Hello { .. } => (None, None),
            Request::Open {
                request_id,
                binding,
                ..
            }
            | Request::Snapshot {
                request_id,
                binding,
                ..
            }
            | Request::Close {
                request_id,
                binding,
                ..
            } => (
                Some(*request_id),
                valid_binding(binding).then(|| binding.clone()),
            ),
        };
        match process(request, received, &mut session, output) {
            Ok(true) => return Ok(()),
            Ok(false) => {}
            Err(code) => {
                return Err(Failure {
                    request_id,
                    binding,
                    code,
                });
            }
        }
    }
}

fn process(
    request: Request,
    received: Instant,
    session: &mut Session,
    output: &mut impl Write,
) -> Result<bool, FaultCode> {
    let remaining_work_ms = match &request {
        Request::Hello {
            remaining_work_ms, ..
        }
        | Request::Open {
            remaining_work_ms, ..
        }
        | Request::Snapshot {
            remaining_work_ms, ..
        }
        | Request::Close {
            remaining_work_ms, ..
        } => *remaining_work_ms,
    };
    let mut work = Budget::new(received, remaining_work_ms)?;
    work.check_work()?;
    match request {
        Request::Hello { protocol, .. } => {
            if session.ready {
                return Err(FaultCode::BadRequest);
            }
            if protocol != PROTOCOL {
                return Err(FaultCode::UnsupportedVersion);
            }
            send(
                output,
                &Reply::Ready {
                    protocol: PROTOCOL,
                    formats: vec!["claude".to_owned()],
                },
                &mut work,
            )?;
            session.ready = true;
        }
        Request::Open {
            request_id,
            binding,
            format,
            path,
            ..
        } => {
            check_request(session, request_id)?;
            if session.source.is_some() || !valid_binding(&binding) {
                return Err(FaultCode::BadRequest);
            }
            if format != "claude" {
                return Err(FaultCode::UnsupportedFormat);
            }
            let path = Path::new(&path);
            if !path.is_absolute() {
                return Err(FaultCode::BadRequest);
            }
            work.check_work()?;
            // Reject ordinary non-files before opening, then verify the actual bound handle.
            if !path.metadata().map_err(|_| FaultCode::Io)?.is_file() {
                return Err(FaultCode::BadRequest);
            }
            work.check_work()?;
            let file = File::open(path).map_err(|_| FaultCode::Io)?;
            work.check_work()?;
            if !file.metadata().map_err(|_| FaultCode::Io)?.is_file() {
                return Err(FaultCode::BadRequest);
            }
            send(
                output,
                &Reply::Opened {
                    request_id,
                    binding: binding.clone(),
                },
                &mut work,
            )?;
            session.source = Some(Source { file, binding });
        }
        Request::Snapshot {
            request_id,
            binding,
            start_byte,
            end_byte,
            end_policy,
            ..
        } => {
            check_request(session, request_id)?;
            let source = bound_source(session, &binding)?;
            snapshot(
                source, request_id, start_byte, end_byte, end_policy, output, &mut work,
            )?;
        }
        Request::Close {
            request_id,
            binding,
            ..
        } => {
            check_request(session, request_id)?;
            bound_source(session, &binding)?;
            drop(session.source.take());
            send(
                output,
                &Reply::Closed {
                    request_id,
                    binding,
                },
                &mut work,
            )?;
            return Ok(true);
        }
    }
    Ok(false)
}

fn check_request(session: &mut Session, request_id: u64) -> Result<(), FaultCode> {
    if !session.ready
        || session
            .last_request_id
            .is_some_and(|previous| request_id <= previous)
    {
        return Err(FaultCode::BadRequest);
    }
    session.last_request_id = Some(request_id);
    Ok(())
}

fn valid_binding(binding: &Binding) -> bool {
    !binding.source_id.is_empty() && binding.source_id.len() <= MAX_SOURCE_ID_BYTES
}

fn bound_source<'a>(
    session: &'a mut Session,
    binding: &Binding,
) -> Result<&'a mut Source, FaultCode> {
    let source = session.source.as_mut().ok_or(FaultCode::BadRequest)?;
    if &source.binding != binding {
        return Err(FaultCode::BindingMismatch);
    }
    Ok(source)
}

fn snapshot(
    source: &mut Source,
    request_id: u64,
    start_byte: u64,
    end_byte: Option<u64>,
    end_policy: EndPolicy,
    output: &mut impl Write,
    work: &mut Budget,
) -> Result<(), FaultCode> {
    work.check_work()?;
    let length = source.file.metadata().map_err(|_| FaultCode::Io)?.len();
    let end_byte = end_byte.unwrap_or(length);
    if start_byte > end_byte || end_byte > length {
        return Err(FaultCode::InvalidRange);
    }
    work.check_work()?;
    if start_byte != 0 {
        source
            .file
            .seek(SeekFrom::Start(start_byte - 1))
            .map_err(|_| FaultCode::Io)?;
        let mut previous = [0];
        read_source(&mut source.file, &mut previous, work)?;
        if previous[0] != b'\n' {
            return Err(FaultCode::InvalidRange);
        }
    }
    work.check_work()?;
    source
        .file
        .seek(SeekFrom::Start(start_byte))
        .map_err(|_| FaultCode::Io)?;
    let mut reader = SnapshotReader::new(
        SourceMeta {
            source_id: source.binding.source_id.clone(),
        },
        start_byte,
        end_policy,
    );
    let mut remaining = end_byte - start_byte;
    let mut chunk = [0; MAX_PART_BYTES];
    let chunk_bytes = u64::try_from(chunk.len()).map_err(|_| FaultCode::Core)?;
    while remaining != 0 {
        let count = usize::try_from(remaining.min(chunk_bytes)).map_err(|_| FaultCode::Core)?;
        read_source(&mut source.file, &mut chunk[..count], work)?;
        reader.push(&chunk[..count], work).map_err(core_fault)?;
        remaining = remaining
            .checked_sub(u64::try_from(count).map_err(|_| FaultCode::Core)?)
            .ok_or(FaultCode::Core)?;
    }
    work.check_work()?;
    let snapshot = reader.finish(work).map_err(core_fault)?;
    work.check_work()?;
    let payload = serde_json::to_vec(&snapshot).map_err(|_| FaultCode::Encode)?;
    work.check_work()?;
    let payload_bytes = u64::try_from(payload.len()).map_err(|_| FaultCode::Encode)?;
    send(
        output,
        &Reply::Begin {
            request_id,
            binding: source.binding.clone(),
        },
        work,
    )?;
    let mut part_count = 0_u64;
    for bytes in payload.chunks(MAX_PART_BYTES) {
        send(
            output,
            &Reply::Part {
                request_id,
                binding: source.binding.clone(),
                part_index: part_count,
                bytes: bytes.to_vec(),
            },
            work,
        )?;
        part_count = part_count.checked_add(1).ok_or(FaultCode::Encode)?;
    }
    send(
        output,
        &Reply::Commit {
            request_id,
            binding: source.binding.clone(),
            part_count,
            payload_bytes,
        },
        work,
    )
}

fn read_source(file: &mut File, bytes: &mut [u8], work: &mut Budget) -> Result<(), FaultCode> {
    work.check_work()?;
    file.read_exact(bytes).map_err(|error| {
        if error.kind() == io::ErrorKind::UnexpectedEof {
            FaultCode::InputChanged
        } else {
            FaultCode::Io
        }
    })?;
    work.check_work()
}

fn core_fault(error: SnapshotError) -> FaultCode {
    match error {
        SnapshotError::OffsetOverflow => FaultCode::Core,
        SnapshotError::Deadline => FaultCode::Deadline,
    }
}

fn read_frame(input: &mut impl Read) -> Result<Option<Vec<u8>>, FaultCode> {
    let mut prefix = [0; 4];
    match input.read_exact(&mut prefix[..1]) {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(_) => return Err(FaultCode::Io),
    }
    input
        .read_exact(&mut prefix[1..])
        .map_err(frame_read_fault)?;
    let length = usize::try_from(u32::from_be_bytes(prefix)).map_err(|_| FaultCode::BadFrame)?;
    if !(1..=MAX_FRAME_BYTES).contains(&length) {
        return Err(FaultCode::BadFrame);
    }
    let mut bytes = vec![0; length];
    input.read_exact(&mut bytes).map_err(frame_read_fault)?;
    Ok(Some(bytes))
}

fn frame_read_fault(error: io::Error) -> FaultCode {
    if error.kind() == io::ErrorKind::UnexpectedEof {
        FaultCode::BadFrame
    } else {
        FaultCode::Io
    }
}

fn send(output: &mut impl Write, reply: &Reply, work: &mut Budget) -> Result<(), FaultCode> {
    work.check_work()?;
    let bytes = serde_json::to_vec(reply).map_err(|_| FaultCode::Encode)?;
    work.check_work()?;
    // The caller retains its original deadline and kills/reaps blocked system calls.
    write_frame(output, &bytes)
}

fn write_frame(output: &mut impl Write, bytes: &[u8]) -> Result<(), FaultCode> {
    if !(1..=MAX_FRAME_BYTES).contains(&bytes.len()) {
        return Err(FaultCode::Encode);
    }
    let length = u32::try_from(bytes.len()).map_err(|_| FaultCode::Encode)?;
    output
        .write_all(&length.to_be_bytes())
        .and_then(|()| output.write_all(bytes))
        .and_then(|()| output.flush())
        .map_err(|_| FaultCode::Io)
}
