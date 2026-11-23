//! One validator and cancellation fence for saved and native route transactions.
use super::*;

impl Connection {
    fn id(&mut self) -> Result<u64> {
        self.next = self.next.checked_add(1).ok_or(Fault::Core)?;
        Ok(self.next)
    }
    async fn send(&mut self, request: Request, budget: Budget) -> Result<()> {
        budget
            .run(v2::write_frame(
                self.input.as_mut().ok_or(Fault::Core)?,
                &request,
            ))
            .await?;
        Ok(())
    }
    async fn reply(&mut self, budget: Budget) -> Result<Reply> {
        Ok(budget
            .run(async { self.frames.recv().await.ok_or(Code::Io)? })
            .await?)
    }
    pub(super) async fn open(&mut self, format: &str, path: &Path, budget: Budget) -> Result<()> {
        self.send(
            Request::Hello {
                protocol: v2::PROTOCOL,
                remaining_work_ms: budget.remaining_ms()?,
            },
            budget,
        )
        .await?;
        match self.reply(budget).await? {
            Reply::Ready { protocol, formats } => {
                if protocol != v2::PROTOCOL {
                    return Err(Fault::UnsupportedProtocol);
                }
                if !formats.iter().any(|f| f == format) {
                    return Err(Fault::Core);
                }
            }
            _ => return Err(Fault::UnsupportedProtocol),
        }
        let id = self.id()?;
        self.send(
            Request::Open {
                request_id: id,
                binding: self.binding.clone(),
                format: format.into(),
                path: path.to_str().ok_or(Fault::Core)?.into(),
                remaining_work_ms: budget.remaining_ms()?,
            },
            budget,
        )
        .await?;
        match self.reply(budget).await? {
            Reply::Opened {
                request_id,
                binding,
            } if request_id == id && binding == self.binding => Ok(()),
            Reply::Fault { code, .. } => Err(code.into()),
            _ => Err(Fault::BindingMismatch),
        }
    }
    pub(super) async fn run(mut self, mut orders: mpsc::UnboundedReceiver<Order>) {
        loop {
            let order = tokio::select! {
                biased;
                _ = self.frames.recv() => {
                    orders.close();
                    break;
                }
                order = orders.recv() => order,
            };
            let Some(order) = order else {
                break;
            };
            match order {
                Order::Start {
                    operation,
                    budget,
                    begun,
                    done,
                } => match self.read(operation, budget, begun, &mut orders).await {
                    ReadResult::Done(result) => {
                        let _ = done.send(result);
                    }
                    ReadResult::Broken(fault) => {
                        orders.close();
                        let _ = done.send(Err(fault));
                        let cleanup = Budget::new(Duration::from_secs(1)).unwrap_or(budget);
                        let _ = self.cleanup(cleanup).await;
                        return;
                    }
                },
                Order::Cancel { .. } => {}
            }
        }
        if let Ok(budget) = Budget::new(Duration::from_secs(1)) {
            let _ = self.cleanup(budget).await;
        }
    }
    async fn read(
        &mut self,
        operation: Operation,
        budget: Budget,
        begun: Answer<u64>,
        orders: &mut mpsc::UnboundedReceiver<Order>,
    ) -> ReadResult {
        let started = async {
            let id = self.id()?;
            let request = Request::Native {
                request_id: id,
                binding: self.binding.clone(),
                operation,
                remaining_work_ms: budget.remaining_ms()?,
            };
            self.send(request, budget).await?;
            let tag = match self.reply(budget).await? {
                Reply::Begin { tag } => tag,
                Reply::Fault {
                    request_id,
                    binding,
                    code,
                } if request_id == Some(id) && binding.as_ref() == Some(&self.binding) => {
                    return Err(code.into());
                }
                _ => return Err(Fault::Core),
            };
            Ok((id, tag))
        }
        .await;
        let (id, tag) = match started {
            Ok(value) => value,
            Err(fault) => {
                let _ = begun.send(Err(fault));
                return ReadResult::Done(Err(fault));
            }
        };
        if tag.request_id != id || tag.binding != self.binding || tag.transport_id <= self.transport
        {
            // Finish the rejected transaction before allowing the next control operation.
            let result = loop {
                match self.reply(budget).await {
                    Ok(Reply::Part { .. }) => {}
                    Ok(Reply::Commit { .. } | Reply::Fault { .. }) => {
                        break ReadResult::Done(Err(Fault::BindingMismatch));
                    }
                    _ => break ReadResult::Broken(Fault::BindingMismatch),
                }
            };
            let _ = begun.send(Err(Fault::BindingMismatch));
            return result;
        }
        let mut failure = None;
        self.transport = tag.transport_id;
        let _ = begun.send(Ok(id));
        let (mut parts, mut bytes) = (0u64, Vec::new());
        loop {
            tokio::select! {
                biased;
                order = orders.recv() => match order {
                    Some(Order::Cancel { target, budget: cancel_budget }) if target == id => {
                        return if self.cancel(&tag, cancel_budget).await.is_err() {
                            ReadResult::Broken(Fault::Core)
                        } else {
                            ReadResult::Done(Err(Fault::Cancelled))
                        };
                    }
                    Some(Order::Start { begun, done, .. }) => { let _ = begun.send(Err(Fault::Core)); let _ = done.send(Err(Fault::Core)); }
                    Some(Order::Cancel { .. }) => {}
                    None => return ReadResult::Broken(Fault::Cancelled),
                },
                _ = tokio::time::sleep_until(budget.0) => return self.expired_read(&tag).await,
                frame = self.frames.recv() => {
                    let reply = match frame { Some(Ok(reply)) => reply, Some(Err(code)) => return ReadResult::Broken(code.into()), None => return ReadResult::Broken(Fault::Core) };
                    match reply {
                        Reply::Part { tag: actual, part_index, bytes: part } => {
                            if actual != tag { failure = Some(Fault::BindingMismatch); }
                            if part_index != parts || part.is_empty() || part.len() > MAX_PART_BYTES { failure = Some(Fault::Core); }
                            parts = match parts.checked_add(1) { Some(n) => n, None => return ReadResult::Broken(Fault::Core) };
                            bytes.extend(part);
                        }
                        Reply::Commit { tag: actual, part_count, payload_bytes } => {
                            if actual != tag { failure = Some(Fault::BindingMismatch); }
                            if part_count != parts || payload_bytes != bytes.len() as u64 { failure = Some(Fault::Core); }
                            let result = match failure { Some(fault) => Err(fault), None => serde_json::from_slice(&bytes).map_err(|_| Fault::Core) };
                            if budget.check().is_err() { return ReadResult::Done(Err(Fault::Core)); }
                            return ReadResult::Done(result);
                        }
                        Reply::Fault { request_id, binding, code } if request_id == Some(id) && binding.as_ref() == Some(&self.binding) => return ReadResult::Done(Err(code.into())),
                        _ => return ReadResult::Broken(Fault::Core),
                    }
                }
            }
        }
    }
    async fn expired_read(&mut self, tag: &Tag) -> ReadResult {
        let Ok(cleanup) = Budget::new(Duration::from_secs(1)) else {
            return ReadResult::Broken(Fault::Core);
        };
        // Drain the terminal reply before reuse.
        loop {
            match self.reply(cleanup).await {
                Ok(Reply::Part { tag: actual, .. }) if actual == *tag => {}
                Ok(Reply::Commit { tag: actual, .. }) if actual == *tag => {
                    return ReadResult::Done(Err(Fault::Core));
                }
                Ok(Reply::Fault {
                    request_id,
                    binding,
                    ..
                }) if request_id == Some(tag.request_id)
                    && binding.as_ref() == Some(&self.binding) =>
                {
                    return ReadResult::Done(Err(Fault::Core));
                }
                _ => return ReadResult::Broken(Fault::Core),
            }
        }
    }
    async fn cancel(&mut self, tag: &Tag, budget: Budget) -> Result<()> {
        let id = self.id()?;
        self.send(
            Request::Cancel {
                request_id: id,
                binding: self.binding.clone(),
                target_request_id: tag.request_id,
                remaining_work_ms: budget.remaining_ms()?,
            },
            budget,
        )
        .await?;
        let mut fenced = false;
        loop {
            match self.reply(budget).await? {
                Reply::Part { tag: actual, .. } | Reply::Commit { tag: actual, .. }
                    if actual == *tag => {}
                Reply::Fenced { tag: actual } if actual == *tag && !fenced => fenced = true,
                Reply::Cancelled {
                    request_id,
                    binding,
                    target_request_id,
                    transport_id,
                    ..
                } if fenced
                    && request_id == id
                    && binding == self.binding
                    && target_request_id == tag.request_id
                    && transport_id == tag.transport_id =>
                {
                    return Ok(());
                }
                _ => return Err(Fault::BindingMismatch),
            }
        }
    }
    pub(super) async fn cleanup(mut self, budget: Budget) -> Result<()> {
        self.input.take();
        self.frames.close();
        let _ = self.child.start_kill();
        let result = budget
            .run(async { self.child.wait().await.map(|_| ()).map_err(|_| Code::Io) })
            .await;
        if result.is_err() {
            let _ = self.child.start_kill();
            let _ = self.child.wait().await;
        }
        // Release queued frames so the owned stdout task can reach EOF.
        while self.frames.recv().await.is_some() {}
        let (out, err) = tokio::join!(self.stdout, self.stderr);
        out.map_err(|_| Fault::Core)??;
        err.map_err(|_| Fault::Core)??;
        result?;
        budget.check()?;
        Ok(())
    }
}
enum ReadResult {
    Done(Result<Value>),
    Broken(Fault),
}
