//! Validated controls keep the canonical live mutations in one session.
use super::*;
impl Session {
    pub(super) fn control(&mut self, request: Control, mut budget: Budget) -> Result<bool> {
        let (id, binding, _) = request.context();
        if let Some(id) = id {
            if !self.ready || self.last.is_some_and(|last| id <= last) {
                return Err("bad_request");
            }
            self.last = Some(id);
            let b = binding.ok_or("bad_request")?;
            if b.source_id.is_empty() || b.source_id.len() > MAX_SOURCE_ID_BYTES {
                return Err("bad_request");
            }
            if !matches!(request, Control::Open { .. })
                && self.source.as_ref().is_none_or(|s| s.binding != *b)
            {
                return Err("binding_mismatch");
            }
            if self
                .saved
                .as_ref()
                .is_some_and(|s| matches!(s.terminal, Some(Terminal::Close { .. })))
            {
                return Err("bad_request");
            }
        }
        if !matches!(request, Control::Cancel { .. } | Control::Close { .. }) {
            budget.check_now()?;
        }
        match request {
            Control::Hello { protocol, .. } => {
                if self.ready || protocol != 1 {
                    return Err("bad_request");
                }
                self.send(vec![Reply::Ready { protocol: 1 }], budget)?;
                self.ready = true;
            }
            Control::Open {
                request_id,
                binding,
                format,
                path,
                ..
            } => {
                if self.source.is_some() || !Path::new(&path).is_absolute() {
                    return Err("bad_request");
                }
                let native = NativeFormat::from_name(&format).ok_or("bad_request")?;
                let mut held = File::open(&path).map_err(|_| "io")?;
                budget.check_now()?;
                let meta = held.metadata().map_err(|_| "io")?;
                budget.check_now()?;
                if !meta.is_file() {
                    return Err("bad_request");
                }
                let kernel_cursor_byte = held
                    .seek(SeekFrom::Start(KERNEL_CURSOR_WITNESS_BYTE))
                    .map_err(|_| "io")?;
                let follow = SourceFollow::with_open_path(
                    held.try_clone().map_err(|_| "io")?,
                    native,
                    &path,
                );
                let (device, inode) = (meta.dev().to_string(), meta.ino().to_string());
                self.send(
                    vec![Reply::Opened {
                        request_id,
                        binding: binding.clone(),
                        device_id: device.clone(),
                        inode_id: inode.clone(),
                        kernel_cursor_byte,
                    }],
                    budget,
                )?;
                self.source = Some(Source {
                    held,
                    follow,
                    binding,
                    path,
                    format,
                    device,
                    inode,
                });
            }
            Control::Bind {
                request_id,
                binding,
                reader,
                from_byte,
                ..
            } => {
                let source = self.source.as_mut().ok_or("bad_request")?;
                let batch = source
                    .follow
                    .bind_reader(reader, from_byte, &mut budget)
                    .map_err(source_fault)?;
                let bytes = serde_json::to_vec(&batch).map_err(|_| "encode")?;
                self.send(
                    payload(
                        &Tag {
                            binding,
                            request_id,
                            transport_id: 0,
                        },
                        &bytes,
                    )?,
                    budget,
                )?;
            }
            Control::Advance {
                request_id,
                binding,
                ..
            } => {
                let source = self.source.as_mut().ok_or("bad_request")?;
                self.advance_failed = true;
                let batch = source.follow.advance(&mut budget).map_err(source_fault)?;
                let bytes = serde_json::to_vec(&batch).map_err(|_| "encode")?;
                self.send(
                    payload(
                        &Tag {
                            binding,
                            request_id,
                            transport_id: 0,
                        },
                        &bytes,
                    )?,
                    budget,
                )?;
                self.advance_failed = false;
            }
            Control::Read {
                request_id,
                binding,
                start_byte,
                end_byte,
                end_policy,
                guard,
                ..
            } => {
                if self.saved.is_some() {
                    return Err("bad_request");
                }
                let source = self.source.as_ref().ok_or("bad_request")?;
                if start_byte != 0 || end_byte > source.held.metadata().map_err(|_| "io")?.len() {
                    return Err("invalid_range");
                }
                budget.check_now()?;
                self.transport = self.transport.checked_add(1).ok_or("core")?;
                let tag = Tag {
                    binding,
                    request_id,
                    transport_id: self.transport,
                };
                let job = Job {
                    tag: tag.clone(),
                    format: source.format.clone(),
                    original_open_path: source.path.clone(),
                    start_byte,
                    end_byte,
                    end_policy,
                    guard,
                    work: budget.remaining()?,
                };
                let running = supervisor::spawn(
                    source.held.try_clone().map_err(|_| "io")?,
                    job,
                    budget,
                    self.events.clone(),
                )?;
                self.saved = Some(Saved {
                    tag,
                    running: Some(running),
                    budget,
                    terminal: None,
                    cleanup: None,
                    output_failed: None,
                    recovery_queued: false,
                });
            }
            Control::Cancel {
                request_id,
                target_request_id,
                ..
            } => {
                let saved = self.saved.as_mut().ok_or("bad_request")?;
                if saved.tag.request_id != target_request_id
                    || saved.terminal.is_some()
                    || saved.cleanup.is_some()
                {
                    return Err("bad_request");
                }
                // The authority's terminal decision is made before the supervisor releases a guard.
                saved.terminal = Some(Terminal::Cancel { request_id, budget });
                if let Some(running) = &saved.running {
                    // A closed receiver means owned Done is already on its way.
                    let _ = running.tx.send(Order::Cancel(budget));
                }
            }
            Control::Close { request_id, .. } => return self.shutdown(Some(request_id), budget),
        }
        Ok(false)
    }
    pub(super) fn shutdown(&mut self, request_id: Option<u64>, budget: Budget) -> Result<bool> {
        if let Some(saved) = self.saved.as_mut() {
            saved.terminal = Some(Terminal::Close { request_id, budget });
            if saved.cleanup.is_some() {
                // The terminal output receipt already queued by the writer finishes this Close.
                return Ok(false);
            }
            let tag = saved.tag.clone();
            if let Some(running) = &saved.running {
                let _ = running.tx.send(Order::Close(budget));
            }
            self.send_saved(vec![Reply::Fenced { tag }], budget, false, false)?;
            Ok(false)
        } else {
            if let Some(id) = request_id {
                let binding = self.release_source()?;
                self.send(
                    vec![Reply::Closed {
                        request_id: id,
                        binding,
                    }],
                    budget,
                )?;
            } else {
                self.source.take();
            }
            Ok(true)
        }
    }
}
fn source_fault(error: SourceError) -> &'static str {
    match error {
        SourceError::InvalidRange => "invalid_range",
        SourceError::BindingMismatch => "binding_mismatch",
        SourceError::Io => "io",
        SourceError::InputChanged => "input_changed",
        SourceError::Work(e) => crate::executor::core_fault(e),
    }
}
