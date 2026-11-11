//! The one stdout owner reports intact-boundary saved failures without ending live state.
use super::protocol::*;
use tokio::{
    io::AsyncWrite,
    sync::{mpsc, oneshot},
};

pub struct SavedOutput {
    pub tag: Tag,
    pub terminal: bool,
    pub recovery: bool,
}
pub struct Output {
    pub frames: Vec<Reply>,
    pub budget: Budget,
    pub saved: Option<SavedOutput>,
    pub ack: Option<oneshot::Sender<Result<()>>>,
}
pub enum Event {
    Failed { tag: Tag, code: &'static str },
    Delivered(Tag),
    Fatal(&'static str),
}

pub async fn run(
    mut output: impl AsyncWrite + Unpin,
    mut input: mpsc::UnboundedReceiver<Output>,
    events: mpsc::UnboundedSender<Event>,
) -> Result<()> {
    // One saved job is admitted at a time and transport IDs strictly increase.
    let mut failed_transport = 0;
    while let Some(bundle) = input.recv().await {
        if bundle
            .saved
            .as_ref()
            .is_some_and(|s| !s.recovery && s.tag.transport_id <= failed_transport)
        {
            continue;
        }
        let mut failed = false;
        for frame in &bundle.frames {
            match write_frame_at_boundary(&mut output, frame, bundle.budget).await {
                Ok(()) => {}
                Err(FrameError::Boundary(code)) => {
                    if let Some(saved) = &bundle.saved
                        && !saved.recovery
                    {
                        failed_transport = failed_transport.max(saved.tag.transport_id);
                        events
                            .send(Event::Failed {
                                tag: saved.tag.clone(),
                                code,
                            })
                            .map_err(|_| "io")?;
                        failed = true;
                        break;
                    }
                    // Live state cannot continue after its mutated result is lost.
                    let (id, binding) = context(frame);
                    let fault = fault(id, binding, code);
                    let _ = write_frame_at_boundary(&mut output, &fault, Budget::cleanup()).await;
                    if let Some(ack) = bundle.ack {
                        let _ = ack.send(Err(code));
                    }
                    let _ = events.send(Event::Fatal(code));
                    return Err(code);
                }
                Err(FrameError::Transport(code)) => {
                    // A partial frame is a shared transport failure, not an operation refusal.
                    if let Some(ack) = bundle.ack {
                        let _ = ack.send(Err(code));
                    }
                    let _ = events.send(Event::Fatal(code));
                    return Err(code);
                }
            }
        }
        if failed {
            continue;
        }
        if let Some(saved) = bundle.saved
            && saved.terminal
        {
            events.send(Event::Delivered(saved.tag)).map_err(|_| "io")?;
        }
        if let Some(ack) = bundle.ack {
            let _ = ack.send(Ok(()));
        }
    }
    Ok(())
}
fn context(
    frame: &Reply,
) -> (
    Option<u64>,
    Option<swarm::view::transcript::worker_protocol::Binding>,
) {
    if let Some(tag) = frame.tag() {
        return (Some(tag.request_id), Some(tag.binding.clone()));
    }
    match frame {
        Reply::Opened {
            request_id,
            binding,
            ..
        }
        | Reply::Closed {
            request_id,
            binding,
        }
        | Reply::Cancelled {
            request_id,
            binding,
            ..
        } => (Some(*request_id), Some(binding.clone())),
        Reply::Fault {
            request_id,
            binding,
            ..
        } => (*request_id, binding.clone()),
        _ => (None, None),
    }
}
