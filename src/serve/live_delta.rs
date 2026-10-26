//! Server live delta.

use super::*;

impl LiveRows {
    pub(super) fn delta(&mut self, key: &str) -> Frame {
        let mut upsert = Vec::new();
        let mut remove = Vec::new();
        if !self.initialized
            || self.rebuild
            || (self.start == 0 && self.records.len() < 30 && !self.dirty.is_empty())
        {
            let rows = self.rows_full();
            let old: Vec<_> = if self.initialized {
                self.chunks
                    .values()
                    .flat_map(|chunk| chunk.rows.iter())
                    .chain(self.last_queued.iter())
                    .collect()
            } else {
                self.last_rows.iter().collect()
            };
            let previous: HashMap<_, _> = old.iter().map(|row| (&row.id, *row)).collect();
            upsert.extend(
                rows.iter()
                    .filter(|row| previous.get(&row.id).is_none_or(|old| *old != *row))
                    .cloned(),
            );
            let ids: HashSet<_> = rows.iter().map(|row| &row.id).collect();
            remove.extend(
                old.iter()
                    .filter(|row| !ids.contains(&row.id))
                    .map(|row| row.id.clone()),
            );
            self.remember_shown(&rows);
            self.prepare(rows);
            self.last_rows.clear();
        } else {
            let mut dirty: Vec<_> = self.dirty.drain().collect();
            dirty.sort_unstable();
            for id in dirty {
                let chunk = &self.chunks[&id];
                let mut positions = chunk.records.clone();
                positions.sort_unstable();
                let records: Vec<_> = positions
                    .iter()
                    .filter_map(|key| self.records.get(key).cloned())
                    .collect();
                let mut rows = view::view_rows_with_context(
                    &records,
                    true,
                    self.is_codex,
                    &self.shown,
                    Some(chunk.context),
                );
                // Queued texts come from the whole queue below, not from one chunk.
                rows.retain(|row| row.kind != view::RowKind::Queued);
                upsert.extend(rows.iter().filter(|row| !chunk.rows.contains(row)).cloned());
                remove.extend(
                    chunk
                        .rows
                        .iter()
                        .filter(|row| !rows.iter().any(|new| new.id == row.id))
                        .map(|row| row.id.clone()),
                );
                self.remember_shown(&rows);
                self.chunks.get_mut(&id).unwrap().rows = rows;
            }
            if self.queue_dirty {
                let rows: Vec<_> =
                    view::view_rows(&self.pending_queue, true, self.is_codex, &self.shown)
                        .into_iter()
                        .filter(|row| row.kind == view::RowKind::Queued)
                        .collect();
                upsert.extend(
                    rows.iter()
                        .filter(|row| !self.last_queued.contains(row))
                        .cloned(),
                );
                remove.extend(
                    self.last_queued
                        .iter()
                        .filter(|row| !rows.iter().any(|new| new.id == row.id))
                        .map(|row| row.id.clone()),
                );
                self.remember_shown(&rows);
                self.last_queued = rows;
                self.queue_dirty = false;
            }
        }
        let usage = self.usage.value();
        let mut payload =
            json!({"key": key, "upsert": upsert, "remove": remove, "end_offset": self.end});
        if usage != self.last_usage {
            payload["usage"] = json!(usage);
        }
        self.last_usage = usage;
        self.compact();
        Frame::new("rows", payload, Some(key))
    }
    pub(super) fn reindex(&mut self) {
        let initial = self
            .records
            .first_key_value()
            .and_then(|(key, _)| self.contexts.get(key))
            .copied();
        let usage = self.usage.clone();
        let queue_lost = self.pending_queue.iter().any(|record| {
            !self
                .records
                .values()
                .any(|cached| cached.source_id == record.source_id)
        });
        let pending_queue = std::mem::take(&mut self.pending_queue);
        let queue_cursor = self.queue_cursor;
        let records = std::mem::take(&mut self.records);
        let mut fresh = Self::empty(self.start, self.is_codex);
        if let Some(context) = initial {
            fresh.context = context;
        }
        fresh.end = self.end;
        fresh.shown = std::mem::take(&mut self.shown);
        fresh.last_usage = self.last_usage.take();
        fresh.clear_row = self.clear_row.take();
        if queue_lost {
            fresh.pending_queue = pending_queue;
            fresh.queue_cursor = queue_cursor;
        }
        for (key, record) in records {
            fresh.usage.ingest(&record.event);
            fresh.contexts.insert(key, fresh.context);
            fresh.records.insert(key, record);
            fresh.update_queue(key);
            fresh.append(key);
        }
        if usage.has_report() {
            fresh.usage = usage;
        }
        *self = fresh;
    }
    pub(super) fn rows_full(&self) -> Vec<Row> {
        let records = self.records.values().cloned().collect::<Vec<_>>();
        let compacted = self
            .records
            .first_key_value()
            .is_some_and(|(key, _)| key.0 > self.start);
        let context = compacted.then(|| self.contexts[self.records.first_key_value().unwrap().0]);
        let mut rows = view::view_rows_with_context(
            &records,
            self.start > 0 || compacted,
            self.is_codex,
            &self.shown,
            context,
        );
        rows.retain(|row| row.kind != view::RowKind::Queued);
        // The queue edits already show as record rows above; only the queued texts come from here.
        rows.extend(
            view::view_rows(&self.pending_queue, true, self.is_codex, &self.shown)
                .into_iter()
                .filter(|row| row.kind == view::RowKind::Queued),
        );
        if compacted && let Some(divider) = &self.clear_row {
            rows.insert(0, divider.clone());
        }
        rows
    }
}

#[cfg(test)]
#[path = "live_delta/tests.rs"]
// Run locally with `cargo test --features serve performance_tests -- --ignored --test-threads=1`.
mod performance_tests;
