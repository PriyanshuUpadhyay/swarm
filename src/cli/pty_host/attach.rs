//! A viewer of a hosted pane (ADR 0131): the Cmd+J terminal runs `swarm pty attach`, which gets
//! the pane's screen, then its live bytes, and sends its keys and size back.

use super::pane::{COLS, ROWS};
use super::{Children, Hosted, lock};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex};

/// Chunks that wait for a viewer. A viewer that falls this far behind is dropped, so a stalled
/// terminal never stops the pane's reader.
const VIEW_QUEUE: usize = 256;

static NEXT_VIEWER: AtomicU64 = AtomicU64::new(0);

/// The viewers of one pane, each with its id and the queue its writer thread sends.
pub(super) type Viewers = Arc<Mutex<Vec<(u64, SyncSender<Vec<u8>>)>>>;

/// Sends `bytes` to each viewer, and drops a viewer whose queue is full or gone. The caller holds
/// the parser lock, so a new viewer's screen and these bytes never cross.
pub(super) fn show(viewers: &Viewers, bytes: &[u8]) {
    lock(viewers).retain(|(_, queue)| queue.try_send(bytes.to_vec()).is_ok());
}

/// Sets the pane's terminal and parser to `rows` x `cols`; the kernel signals the CLI to redraw.
pub(super) fn resize(master: &std::fs::File, parser: &Mutex<vt100::Parser>, rows: u16, cols: u16) {
    use std::os::fd::AsRawFd;
    let (rows, cols) = (rows.clamp(2, 500), cols.clamp(2, 1000));
    let size = libc::winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    lock(parser).screen_mut().set_size(rows, cols);
    // SAFETY: TIOCSWINSZ reads a local winsize; a closed fd only returns -1.
    unsafe { libc::ioctl(master.as_raw_fd(), libc::TIOCSWINSZ, &size) };
}

fn size_of(request: &serde_json::Value) -> (u16, u16) {
    let field = |key: &str, default: u16| {
        request[key]
            .as_u64()
            .and_then(|value| u16::try_from(value).ok())
            .unwrap_or(default)
    };
    (field("rows", ROWS), field("cols", COLS))
}

/// The `resize` op: one viewer's new size.
pub(super) fn resize_op(request: &serde_json::Value, hosted: &Hosted) {
    let (rows, cols) = size_of(request);
    resize(&hosted.master, &hosted.parser, rows, cols);
}

/// Serves one viewer on `stream` until it leaves or the pane ends. The last viewer out puts the
/// pane back at tmux's detached size, as tmux-swarm's attach does.
pub(super) fn attach(
    stream: UnixStream,
    request: &serde_json::Value,
    children: &Children,
) -> Result<(), String> {
    let n = request["n"].as_u64().ok_or("swarm pty: request has no n")?;
    let (input, viewers, parser, master) = {
        let children = lock(children);
        let hosted = children.get(&n).ok_or("swarm pty: no such pane")?;
        let master = hosted
            .master
            .try_clone()
            .map_err(|error| error.to_string())?;
        (
            hosted.input.clone(),
            hosted.viewers.clone(),
            hosted.parser.clone(),
            master,
        )
    };
    let id = NEXT_VIEWER.fetch_add(1, Ordering::Relaxed);
    let (queue, queued) = std::sync::mpsc::sync_channel::<Vec<u8>>(VIEW_QUEUE);
    {
        let parser = lock(&parser);
        let mut first = b"{\"ok\":true,\"out\":\"\"}\n".to_vec();
        if parser.screen().alternate_screen() {
            first.extend_from_slice(b"\x1b[?1049h");
        }
        first.extend_from_slice(b"\x1b[H\x1b[2J");
        first.extend(parser.screen().state_formatted());
        queue.try_send(first).map_err(|error| error.to_string())?;
        lock(&viewers).push((id, queue));
    }
    let (rows, cols) = size_of(request);
    resize(&master, &parser, rows, cols);
    let mut out = stream.try_clone().map_err(|error| error.to_string())?;
    std::thread::spawn(move || {
        for bytes in queued {
            if out.write_all(&bytes).is_err() {
                break;
            }
        }
        // The viewer reads its end when it was dropped or the pane ended.
        let _ = out.shutdown(std::net::Shutdown::Both);
    });
    let mut stream = stream;
    let _ = stream.set_read_timeout(None);
    let mut chunk = [0; 4096];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => {
                if input.send(chunk[..read].to_vec()).is_err() {
                    break;
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => break,
        }
    }
    let last = {
        let mut viewers = lock(&viewers);
        viewers.retain(|(viewer, _)| *viewer != id);
        viewers.is_empty()
    };
    // Outside the viewers lock, since the reader takes the parser lock before it.
    if last {
        resize(&master, &parser, ROWS, COLS);
    }
    Ok(())
}
