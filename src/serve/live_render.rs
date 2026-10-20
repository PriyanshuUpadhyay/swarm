//! Server live render.

use super::*;

impl LiveRows {
    pub(super) fn rows(&self) -> Vec<Row> {
        if !self.initialized || self.rebuild {
            return self.rows_full();
        }
        self.chunks
            .values()
            .flat_map(|chunk| chunk.rows.clone())
            .chain(self.last_queued.clone())
            .collect()
    }
    pub(super) fn remember_shown(&mut self, rows: &[Row]) {
        for row in rows {
            if row.kind != view::RowKind::Fold {
                self.shown.insert(row.id.clone());
            } else if row.default_open {
                self.shown
                    .extend(row.steps.iter().map(|row| row.id.clone()));
            }
        }
    }
    pub(super) fn prepare(&mut self, rows: Vec<Row>) {
        self.clear_row = rows
            .iter()
            .find(|row| row.kind == view::RowKind::Divider)
            .cloned();
        self.chunks.clear();
        self.record_chunks.clear();
        self.dirty.clear();
        self.next_chunk = 0;
        for queued in &self.last_queued {
            if !rows.iter().any(|row| row.id == queued.id) {
                self.shown.remove(&queued.id);
            }
        }
        self.last_queued = rows
            .iter()
            .filter(|row| row.kind == view::RowKind::Queued)
            .cloned()
            .collect();
        for row in rows
            .into_iter()
            .filter(|row| row.kind != view::RowKind::Queued)
        {
            let step = swarm::view::is_step(&row);
            let can_append = step
                && row.kind != view::RowKind::Fold
                && self.chunks.last_key_value().is_some_and(|(_, chunk)| {
                    chunk.rows.iter().all(swarm::view::is_step)
                        && chunk
                            .rows
                            .iter()
                            .map(|row| {
                                if row.kind == view::RowKind::Fold {
                                    row.steps.len()
                                } else {
                                    1
                                }
                            })
                            .sum::<usize>()
                            < swarm::view::FOLD_STEP_LIMIT
                });
            let id = if can_append {
                *self.chunks.last_key_value().unwrap().0
            } else {
                let id = self.next_chunk;
                self.next_chunk += 1;
                self.chunks.insert(
                    id,
                    LiveChunk {
                        records: Vec::new(),
                        rows: Vec::new(),
                        context: (self.start == 0, self.start > 0),
                    },
                );
                id
            };
            let mut records: Vec<_> = row
                .source_ids
                .iter()
                .filter_map(|source| {
                    let mut pieces = source.strip_prefix("raw-")?.split('-');
                    Some((pieces.next()?.parse().ok()?, pieces.next()?.parse().ok()?))
                })
                .collect();
            if let (Some(&first), Some(&last)) = (records.iter().min(), records.iter().max()) {
                records.extend(
                    self.records
                        .range(first..=last)
                        .filter(|(_, record)| {
                            matches!(
                                view::string(&record.event, "type"),
                                "page" | "turn_started" | "turn_ended"
                            )
                        })
                        .map(|(key, _)| *key),
                );
                records.sort_unstable();
                records.dedup();
            }
            for key in &records {
                self.record_chunks.insert(*key, id);
            }
            let chunk = self.chunks.get_mut(&id).unwrap();
            if chunk.records.is_empty() {
                chunk.context = records
                    .first()
                    .and_then(|key| self.contexts.get(key))
                    .copied()
                    .unwrap_or((self.start == 0, self.start > 0));
            }
            chunk.records.extend(records);
            chunk.rows.push(row);
        }
        self.tail_steps = self.chunks.last_key_value().map_or(0, |(_, chunk)| {
            if chunk
                .rows
                .iter()
                .all(|row| row.kind == view::RowKind::Fold || swarm::view::is_step(row))
            {
                chunk
                    .rows
                    .iter()
                    .map(|row| {
                        if row.kind == view::RowKind::Fold {
                            row.steps.len()
                        } else {
                            1
                        }
                    })
                    .sum()
            } else {
                0
            }
        });
        self.last_prose = self
            .records
            .values()
            .rev()
            .find_map(view::row)
            .and_then(|row| {
                matches!(
                    row.kind,
                    view::RowKind::User | view::RowKind::Assistant | view::RowKind::Thought
                )
                .then_some((row.kind, row.id))
            });
        self.initialized = true;
        self.rebuild = false;
        self.queue_dirty = false;
    }
    pub(super) fn compact(&mut self) {
        // shortcut: one open turn can exceed 2000 records; spill it only if its memory becomes a problem.
        while self.records.len() > ROW_RECORD_LIMIT {
            let Some((&key, _)) = self.records.first_key_value() else {
                break;
            };
            if key >= self.scope_start {
                break;
            }
            if let Some(id) = self.record_chunks.get(&key).copied() {
                let chunk = &self.chunks[&id];
                if chunk.records.iter().any(|key| *key >= self.scope_start) {
                    break;
                }
                let chunk = self.chunks.remove(&id).unwrap();
                for row in &chunk.rows {
                    self.shown.remove(&row.id);
                    for step in &row.steps {
                        self.shown.remove(&step.id);
                    }
                }
                for key in chunk.records {
                    self.records.remove(&key);
                    self.contexts.remove(&key);
                    self.record_chunks.remove(&key);
                }
            } else {
                self.records.remove(&key);
                self.contexts.remove(&key);
            }
        }
    }
}
