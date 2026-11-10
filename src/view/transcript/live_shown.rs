//! Live visibility uses the browser's letter class and UTF-16 comparison units.

use super::{Row, RowKind};

#[rustfmt::skip]
mod letter_class;

pub fn letters(text: &str) -> Vec<u16> {
    text.chars()
        .filter(|c| {
            let point = *c as u32;
            letter_class::LETTER_OR_NUMBER
                .binary_search_by(|&(start, end)| {
                    if point < start {
                        std::cmp::Ordering::Greater
                    } else if point > end {
                        std::cmp::Ordering::Less
                    } else {
                        std::cmp::Ordering::Equal
                    }
                })
                .is_ok()
        })
        .collect::<String>()
        .encode_utf16()
        .collect()
}

pub fn landed(rows: &[Row], text: &str, kind: RowKind) -> bool {
    let letters = letters(text);
    let head = &letters[..letters.len().min(80)];
    head.len() >= 20
        && rows.iter().rev().take(50).any(|row| {
            std::iter::once(row).chain(&row.steps).any(|row| {
                row.kind == kind
                    && self::letters(&row.text)
                        .windows(head.len())
                        .any(|part| part == head)
            })
        })
}

pub fn live_text<'a>(rows: &[Row], text: Option<&'a str>) -> Option<&'a str> {
    text.filter(|text| !letters(text).is_empty() && !landed(rows, text, RowKind::Assistant))
}

pub fn live_thought<'a>(rows: &[Row], thinking: Option<&'a str>) -> Option<&'a str> {
    thinking.filter(|text| !text.is_empty() && !landed(rows, text, RowKind::Thought))
}

pub fn keeps_ended(rows: &[Row], text: Option<&str>, thinking: Option<&str>) -> bool {
    (text.is_some_and(|text| letters(text).len() >= 20) && live_text(rows, text).is_some())
        || live_thought(rows, thinking).is_some()
}

#[derive(Clone, Default, PartialEq)]
pub struct LiveRaw {
    pub text: Option<String>,
    pub thinking: Option<String>,
}

#[derive(Clone, PartialEq)]
pub struct Shown {
    pub raw: LiveRaw,
    pub ended: bool,
    pub hide_text: bool,
    pub hide_thinking: bool,
}

#[derive(Clone, Default)]
pub struct LiveShown {
    raw: LiveRaw,
    ended: bool,
    last: Option<Shown>,
}

impl LiveShown {
    /// A read is supplied only on a changed capture. A rows signal clears a kept ended turn.
    pub fn step(
        &mut self,
        rows: &[Row],
        read: Option<LiveRaw>,
        rows_changed: bool,
    ) -> Option<Shown> {
        if rows_changed && self.ended {
            self.raw = LiveRaw::default();
            self.ended = false;
        }
        if let Some(read) = read
            && (read != self.raw || self.ended)
        {
            if read.text.is_none()
                && read.thinking.is_none()
                && keeps_ended(rows, self.raw.text.as_deref(), self.raw.thinking.as_deref())
            {
                self.ended = true;
            } else {
                self.raw = read;
                self.ended = false;
            }
        }
        let frame = Shown {
            hide_text: self
                .raw
                .text
                .as_deref()
                .is_some_and(|text| live_text(rows, Some(text)).is_none()),
            hide_thinking: self
                .raw
                .thinking
                .as_deref()
                .is_some_and(|text| live_thought(rows, Some(text)).is_none()),
            raw: self.raw.clone(),
            ended: self.ended,
        };
        if self.last.as_ref() == Some(&frame) {
            return None;
        }
        self.last = Some(frame.clone());
        Some(frame)
    }
}
