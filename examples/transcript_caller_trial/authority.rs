//! The authority alone owns the source, live framer, terminal decisions and output writer.
use super::{
    protocol::*,
    supervisor::{self, Event, Order, Running},
};
use std::{
    fs::File,
    io::{Seek, SeekFrom},
    os::{fd::AsFd, unix::fs::MetadataExt},
    path::Path,
};
use swarm::view::transcript::{
    source::NativeFormat,
    source_follow::{SourceError, SourceFollow},
    worker_protocol::{Binding, MAX_SOURCE_ID_BYTES},
};
use tokio::{
    fs::File as AsyncFile,
    sync::{mpsc, oneshot},
    time::Instant,
};

use super::output::{self, Output, SavedOutput};
#[path = "authority/control.rs"]
mod control;
#[path = "authority/events.rs"]
mod events;
struct Source {
    held: File,
    follow: SourceFollow,
    binding: Binding,
    path: String,
    format: String,
    device: String,
    inode: String,
}
#[derive(Clone, Copy)]
enum Terminal {
    Cancel {
        request_id: u64,
        budget: Budget,
    },
    Close {
        request_id: Option<u64>,
        budget: Budget,
    },
}
struct Saved {
    tag: Tag,
    running: Option<Running>,
    budget: Budget,
    terminal: Option<Terminal>,
    cleanup: Option<Result<()>>,
    output_failed: Option<&'static str>,
    recovery_queued: bool,
}
struct Session {
    ready: bool,
    advance_failed: bool,
    last: Option<u64>,
    transport: u64,
    source: Option<Source>,
    saved: Option<Saved>,
    output: mpsc::UnboundedSender<Output>,
    events: mpsc::UnboundedSender<Event>,
}

pub async fn run() -> Result<()> {
    let input = tokio::net::unix::pipe::Receiver::from_owned_fd(
        std::io::stdin()
            .as_fd()
            .try_clone_to_owned()
            .map_err(|_| "io")?,
    )
    .map_err(|_| "io")?;
    let output = File::from(
        std::io::stdout()
            .as_fd()
            .try_clone_to_owned()
            .map_err(|_| "io")?,
    );
    let (control_tx, mut control_rx) = mpsc::unbounded_channel();
    let reader = tokio::spawn(async move {
        let mut input = input;
        loop {
            let r = read_frame(&mut input).await;
            let ended = !matches!(r, Ok(Some(_)));
            if control_tx.send((Instant::now(), r)).is_err() || ended {
                return;
            }
        }
    });
    let (output_tx, output_rx) = mpsc::unbounded_channel::<Output>();
    let (writer_err_tx, mut writer_err_rx) = mpsc::unbounded_channel();
    let writer = tokio::spawn(output::run(
        AsyncFile::from_std(output),
        output_rx,
        writer_err_tx,
    ));
    let (events, mut events_rx) = mpsc::unbounded_channel();
    let mut session = Session {
        ready: false,
        advance_failed: false,
        last: None,
        transport: 0,
        source: None,
        saved: None,
        output: output_tx,
        events,
    };
    let result = 'session: loop {
        tokio::select! {
            biased;
            event=writer_err_rx.recv()=>match event {
                Some(event)=>match session.writer_event(event).await {Ok(true)=>break Ok(()),Ok(false)=>{},Err(e)=>break Err(e)},
                None=>break Err("io"),
            },
            event=events_rx.recv(),if session.saved.is_some()=> {
                match event {Some(event)=>match session.event(event).await {Ok(true)=>break Ok(()),Ok(false)=>{},Err(e)=>break Err(e)},None=>break Err("io")}
            }
            control=control_rx.recv()=> {
                let Some((received,frame))=control else {break Err("io")};
                match frame {
                    Ok(Some(bytes))=> {
                        let request=match serde_json::from_slice::<Control>(&bytes) {
                            Ok(r)=>r,Err(_)=>{let _=session.send(vec![fault(None,None,"bad_request")],Budget::cleanup());continue}
                        };
                        let (id,binding,work)=request.context();
                        let binding=binding.cloned();
                        let controlled=match Budget::new(work,received) {
                            Ok(budget)=>{
                                if matches!(request,Control::Read {..}) {
                                    while session.saved.as_ref().is_some_and(|s|s.cleanup.is_some()) {
                                        // Apply any received ownership event before checking the new request's spent budget.
                                        let event=tokio::select! {
                                            biased;
                                            event=writer_err_rx.recv()=>event,
                                            _=budget.expired()=>break,
                                        };
                                        match event {
                                            Some(event)=>match session.writer_event(event).await {
                                                Ok(true)=>break 'session Ok(()),Ok(false)=>{},Err(e)=>break 'session Err(e),
                                            },
                                            None=>break 'session Err("io"),
                                        }
                                    }
                                }
                                session.control(request,budget)
                            }
                            Err(code)=>Err(code),
                        };
                        match controlled {
                            Ok(true)=>break Ok(()),Ok(false)=>{},
                            Err(code)=>{
                                if let Err(e)=session.send(vec![fault(id,binding,code)],Budget::cleanup()) {break Err(e)}
                                if session.advance_failed {break Err(code)}
                            }
                        }
                    }
                    Ok(None)=> {
                        match session.shutdown(None,Budget::cleanup()) {Ok(true)=>break Ok(()),Ok(false)=>{},Err(e)=>break Err(e)}
                    }
                    Err(e)=>break Err(e),
                }
            }
        }
    };
    // A transport failure still fences, kills and joins the one owned saved executor.
    if let Some(mut saved) = session.saved.take() {
        saved.terminal = Some(Terminal::Close {
            request_id: None,
            budget: Budget::cleanup(),
        });
        if let Some(running) = saved.running.take() {
            let _ = running.tx.send(Order::Close(Budget::cleanup()));
            running.task.await.map_err(|_| "io")?;
        }
    }
    session.source.take();
    drop(session);
    let written = writer.await.map_err(|_| "io")?;
    // Closed was flushed first. Root now drops stdin, which releases the control read.
    // Control EOF/transport failure has the same join ownership; arbitrary blocked I/O is excluded.
    if result.is_err() {
        reader.abort();
    }
    match reader.await {
        Ok(()) => {}
        Err(e) if e.is_cancelled() && result.is_err() => {}
        Err(_) => return Err("io"),
    }
    result.and(written)
}

impl Session {
    fn send(&self, frames: Vec<Reply>, budget: Budget) -> Result<()> {
        budget.check_now()?;
        self.output
            .send(Output {
                frames,
                budget,
                ack: None,
                saved: None,
            })
            .map_err(|_| "io")
    }
    async fn flushed(&self, frame: Reply, budget: Budget) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.output
            .send(Output {
                frames: vec![frame],
                budget,
                ack: Some(tx),
                saved: None,
            })
            .map_err(|_| "io")?;
        rx.await.map_err(|_| "io")?
    }
}
