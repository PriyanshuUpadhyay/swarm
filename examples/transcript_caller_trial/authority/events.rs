//! Saved publication stays pending until the one writer reports its terminal frame.
use super::*;

impl Session {
    pub(super) fn send_saved(
        &self,
        frames: Vec<Reply>,
        budget: Budget,
        terminal: bool,
        recovery: bool,
    ) -> Result<()> {
        let tag = self.saved.as_ref().ok_or("bad_request")?.tag.clone();
        // The writer alone knows whether expiry is before a frame or inside its transport.
        self.output
            .send(Output {
                frames,
                budget,
                saved: Some(SavedOutput {
                    tag,
                    terminal,
                    recovery,
                }),
                ack: None,
            })
            .map_err(|_| "io")
    }
    pub(super) fn release_source(&mut self) -> Result<Binding> {
        let source = self.source.take().ok_or("bad_request")?;
        let binding = source.binding.clone();
        drop(source);
        Ok(binding)
    }
    async fn close_finished(&mut self, request_id: Option<u64>, budget: Budget) -> Result<bool> {
        self.saved.take();
        let binding = self.release_source()?;
        if let Some(id) = request_id {
            if let Err(code) = budget.check_now() {
                self.flushed(fault(Some(id), Some(binding), code), Budget::cleanup())
                    .await?;
                return Err(code);
            }
            self.flushed(
                Reply::Closed {
                    request_id: id,
                    binding,
                },
                budget,
            )
            .await?;
        }
        Ok(true)
    }
    fn recover_saved(&mut self, code: &'static str) -> Result<()> {
        let saved = self.saved.as_mut().ok_or("bad_request")?;
        if saved.cleanup.is_none() || saved.recovery_queued {
            return Ok(());
        }
        saved.recovery_queued = true;
        let tag = saved.tag.clone();
        self.send_saved(
            vec![fault(Some(tag.request_id), Some(tag.binding), code)],
            Budget::cleanup(),
            true,
            true,
        )
    }
    pub(super) async fn writer_event(&mut self, event: output::Event) -> Result<bool> {
        match event {
            output::Event::Fatal(code) => return Err(code),
            output::Event::Delivered(tag) => {
                let Some(saved) = self.saved.as_ref().filter(|s| s.tag == tag) else {
                    return Ok(false);
                };
                saved.cleanup.ok_or("io")??;
                if let Some(Terminal::Close { request_id, budget }) = saved.terminal {
                    return self.close_finished(request_id, budget).await;
                }
                self.saved.take();
            }
            output::Event::Failed { tag, code } => {
                let Some(saved) = self.saved.as_mut().filter(|s| s.tag == tag) else {
                    return Ok(false);
                };
                saved.output_failed = Some(code);
                if let Some(running) = &saved.running {
                    // Fence this job; a closed receiver means Done already owns its cleanup result.
                    let _ = running.tx.send(Order::Close(Budget::cleanup()));
                }
                if saved.cleanup.is_some() {
                    if let Some(Terminal::Close { request_id, budget }) = saved.terminal {
                        return self.close_finished(request_id, budget).await;
                    }
                    self.recover_saved(code)?;
                }
            }
        }
        Ok(false)
    }
    pub(super) async fn event(&mut self, event: Event) -> Result<bool> {
        let saved = self.saved.as_ref().ok_or("bad_request")?;
        match event {
            Event::Descriptor(frame) => {
                let source = self.source.as_ref().ok_or("bad_request")?;
                match &frame {
                    Reply::Descriptor {
                        tag,
                        device_id,
                        inode_id,
                        ..
                    } if *tag == saved.tag
                        && *device_id == source.device
                        && *inode_id == source.inode => {}
                    _ => return Err("input_changed"),
                }
                if saved.terminal.is_none() && saved.output_failed.is_none() {
                    self.send_saved(vec![frame], saved.budget, false, false)?;
                }
            }
            Event::Staged(tag) => {
                if tag != saved.tag {
                    return Err("binding_mismatch");
                }
                if saved.terminal.is_none() && saved.output_failed.is_none() {
                    self.send_saved(vec![Reply::Staged { tag }], saved.budget, false, false)?;
                }
            }
            Event::Fenced(tag) => {
                if tag != saved.tag {
                    return Err("binding_mismatch");
                }
                if let Some(Terminal::Cancel { budget, .. }) = saved.terminal
                    && saved.output_failed.is_none()
                {
                    self.send_saved(vec![Reply::Fenced { tag }], budget, false, false)?;
                }
            }
            Event::Done {
                tag,
                result,
                cleanup,
            } => {
                if tag != saved.tag {
                    return Err("binding_mismatch");
                }
                if let Some(running) = self.saved.as_mut().and_then(|s| s.running.take()) {
                    running.task.await.map_err(|_| "io")?;
                }
                let saved = self.saved.as_mut().ok_or("bad_request")?;
                saved.cleanup = Some(cleanup);
                if let Err(code) = cleanup {
                    let id = match saved.terminal {
                        Some(Terminal::Close {
                            request_id: Some(id),
                            ..
                        }) => id,
                        _ => tag.request_id,
                    };
                    self.send(
                        vec![fault(Some(id), Some(tag.binding), code)],
                        Budget::cleanup(),
                    )?;
                    return Err(code);
                }
                match saved.terminal {
                    Some(Terminal::Close { request_id, budget }) => {
                        return self.close_finished(request_id, budget).await;
                    }
                    Some(Terminal::Cancel { request_id, budget }) => {
                        let failure = saved.output_failed.or_else(|| budget.check_now().err());
                        match (failure, result) {
                            (None, Ok((exit, _))) => self.send_saved(
                                vec![Reply::Cancelled {
                                    request_id,
                                    binding: tag.binding.clone(),
                                    target_request_id: tag.request_id,
                                    transport_id: tag.transport_id,
                                    exit,
                                }],
                                budget,
                                true,
                                false,
                            )?,
                            (Some(code), _) | (None, Err(code)) => self.recover_saved(code)?,
                        }
                    }
                    None => {
                        let failure = saved
                            .output_failed
                            .or_else(|| saved.budget.check_now().err());
                        let budget = saved.budget;
                        match (failure, result) {
                            (None, Ok((exit, mut frames))) if exit.success() => {
                                let cursor = self
                                    .source
                                    .as_mut()
                                    .ok_or("bad_request")?
                                    .held
                                    .stream_position()
                                    .map_err(|_| "io")?;
                                // Commit is last, so an expired trailing diagnostic cannot undo it.
                                frames.insert(
                                    0,
                                    Reply::Position {
                                        tag,
                                        kernel_cursor_byte: cursor,
                                    },
                                );
                                self.send_saved(frames, budget, true, false)?;
                            }
                            (Some(code), _) | (None, Err(code)) => self.recover_saved(code)?,
                            (None, Ok(_)) => self.recover_saved("executor_exit")?,
                        }
                    }
                }
            }
        }
        Ok(false)
    }
}
