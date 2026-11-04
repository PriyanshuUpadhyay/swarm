//! Cached tail selection and page overlap retain their existing server behavior.

use super::*;

#[derive(Clone, Debug)]
pub struct RowPage {
    pub rows: Vec<Row>,
    pub remove: Vec<String>,
    pub usage: Option<view::ChatUsage>,
}

impl ReaderRows {
    pub fn cached_tail(&self, start: u64, end: u64) -> Option<RowPage> {
        if !self.initialized
            || self.rebuild
            || !self.dirty.is_empty()
            || self
                .records
                .first_key_value()
                .is_none_or(|(key, _)| key.0 > start)
            || self.end != end
        {
            return None;
        }
        Some(RowPage {
            rows: self
                .chunks
                .values()
                .filter(|chunk| {
                    self.records.len() <= ROW_RECORD_LIMIT
                        || chunk.records.iter().any(|key| key.0 >= start)
                })
                .flat_map(|chunk| chunk.rows.iter().cloned())
                .chain(self.last_queued.iter().cloned())
                .collect(),
            usage: self.usage.value(),
            remove: Vec::new(),
        })
    }

    pub fn recovery_range(&self) -> Option<(u64, u64)> {
        self.records
            .first_key_value()
            .filter(|(key, _)| key.0 > self.start)
            .map(|_| (self.start, self.end))
    }

    pub fn merge_incoming(&mut self, incoming: Self) {
        let usage = self.usage.clone();
        self.shown.extend(incoming.shown.iter().cloned());
        self.start = self.start.min(incoming.start);
        self.end = self.end.max(incoming.end);
        self.replace_records(incoming);
        self.reindex();
        self.usage = usage;
        let rows = self.rows_full();
        self.prepare(rows);
        self.compact();
    }

    pub fn recover(&mut self, recovery: Self) {
        self.replace_records(recovery);
        self.reindex();
    }

    pub fn merge_page(&mut self, page: Self, before: bool) -> RowPage {
        let previous = self.rows();
        self.start = self.start.min(page.start);
        self.end = self.end.max(page.end);
        self.replace_records(page);
        self.reindex();
        let rows = self.rows();
        let ids: HashSet<_> = rows.iter().map(|row| &row.id).collect();
        let remove = previous
            .iter()
            .filter(|row| !ids.contains(&row.id))
            .map(|row| row.id.clone())
            .collect();
        let usage = self.usage.value();
        let previous: HashMap<_, _> = previous.iter().map(|row| (&row.id, row)).collect();
        let upsert = rows
            .iter()
            .filter(|row| !before || previous.get(&row.id).is_none_or(|old| *old != *row))
            .cloned()
            .collect();
        self.last_usage = usage.clone();
        self.remember_shown(&rows);
        self.prepare(rows.clone());
        self.last_rows = rows;
        self.compact();
        self.last_rows = self.rows();
        RowPage {
            rows: upsert,
            remove,
            usage,
        }
    }

    fn replace_records(&mut self, page: Self) {
        self.unavailable
            .retain(|key| !page.records.contains_key(key));
        self.unavailable.extend(page.unavailable);
        self.records.extend(page.records);
    }
}
