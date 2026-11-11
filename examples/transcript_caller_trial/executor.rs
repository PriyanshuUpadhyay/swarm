//! The saved reader receives a File on stdin and never opens a source path.
use super::protocol::*;
use std::{
    fs::File,
    os::{
        fd::AsFd,
        unix::fs::{FileExt, MetadataExt},
    },
};
use swarm::view::transcript::{
    source::{NativeFormat, SnapshotReader, SourceMeta},
    worker_protocol::MAX_PART_BYTES,
};
use tokio::{fs::File as AsyncFile, io::AsyncReadExt, net::UnixStream, time::Instant};

pub async fn run(text: &str) -> Result<()> {
    let received = Instant::now();
    let job: Job = serde_json::from_str(text).map_err(|_| "bad_request")?;
    let mut budget = Budget::new(job.work, received)?;
    let format = NativeFormat::from_name(&job.format).ok_or("bad_request")?;
    let file = File::from(
        std::io::stdin()
            .as_fd()
            .try_clone_to_owned()
            .map_err(|_| "io")?,
    );
    let meta = file.metadata().map_err(|_| "io")?;
    budget.check_now()?;
    if !meta.is_file() || job.start_byte != 0 || job.end_byte > meta.len() {
        return Err("invalid_range");
    }
    let mut out = AsyncFile::from_std(File::from(
        std::io::stdout()
            .as_fd()
            .try_clone_to_owned()
            .map_err(|_| "io")?,
    ));
    write_frame(
        &mut out,
        &Reply::Descriptor {
            tag: job.tag.clone(),
            device_id: meta.dev().to_string(),
            inode_id: meta.ino().to_string(),
            pid: std::process::id(),
        },
        budget,
    )
    .await?;
    let mut reader = SnapshotReader::with_open_path(
        SourceMeta {
            source_id: job.tag.binding.source_id.clone(),
        },
        job.start_byte,
        job.end_policy,
        format,
        &job.original_open_path,
    );
    let mut offset = job.start_byte;
    let mut buf = vec![0; MAX_PART_BYTES];
    while offset < job.end_byte {
        budget.check_now()?;
        let n = usize::try_from((job.end_byte - offset).min(MAX_PART_BYTES as u64))
            .map_err(|_| "invalid_range")?;
        file.read_exact_at(&mut buf[..n], offset)
            .map_err(|_| "input_changed")?;
        budget.check_now()?;
        reader.push(&buf[..n], &mut budget).map_err(core_fault)?;
        offset += n as u64;
    }
    let snapshot = reader.finish_native(&mut budget).map_err(core_fault)?;
    let bytes = serde_json::to_vec(&snapshot).map_err(|_| "encode")?;
    budget.check_now()?;
    let mut frames = payload(&job.tag, &bytes)?;
    let commit = frames.pop().ok_or("core")?;
    for f in frames {
        write_frame(&mut out, &f, budget).await?
    }
    let mut gate = match &job.guard {
        Guard::None => None,
        Guard::HoldBeforeCommit { gate_path, .. } => {
            let mut g = budget
                .run(async { UnixStream::connect(gate_path).await.map_err(|_| "io") })
                .await?;
            budget
                .run(async {
                    let mut b = [0];
                    g.read_exact(&mut b).await.map_err(|_| "io")?;
                    if b[0] != 1 {
                        Err("bad_request")
                    } else {
                        Ok(())
                    }
                })
                .await?;
            Some(g)
        }
    };
    write_frame(&mut out, &commit, budget).await?;
    if matches!(
        job.guard,
        Guard::HoldBeforeCommit {
            after_commit: AfterCommit::WaitForKill,
            ..
        }
    ) {
        // Root must cause an owned-child kill, not EOF or a timer masquerading as cancellation.
        let mut b = [0];
        let g = gate.as_mut().ok_or("core")?;
        g.read_exact(&mut b).await.map_err(|_| "io")?;
        return Err("bad_request");
    }
    Ok(())
}
pub fn core_fault(e: swarm::view::transcript::source::SnapshotError) -> &'static str {
    match e {
        swarm::view::transcript::source::SnapshotError::Deadline => "deadline",
        _ => "core",
    }
}
