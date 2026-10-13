//! Claude's input cleaner, ported from SwarmCore/Composer/ClaudeInvisibleText.swift.
use super::claude_properties as p;

type Set = &'static [(u32, u32)];
fn contains(set: Set, scalar: Option<char>) -> bool {
    scalar.is_some_and(|c| {
        let value = c as u32;
        let index = set.partition_point(|(_, end)| *end < value);
        set.get(index).is_some_and(|(start, _)| *start <= value)
    })
}
#[derive(Clone, Copy)]
struct Script {
    base: Set,
    mark: Set,
}
const NON_JOINER: Script = Script {
    base: p::NONJOINERSCRIPTS_BASE,
    mark: p::NONJOINERSCRIPTS_MARK,
};
const JOINER: Script = Script {
    base: p::JOINERSCRIPTS_BASE,
    mark: p::JOINERSCRIPTS_MARK,
};
const CURSIVE: Script = Script {
    base: p::CURSIVESCRIPTS_BASE,
    mark: p::CURSIVESCRIPTS_MARK,
};
const SOUTHEAST: Script = Script {
    base: p::SOUTHEASTASIAN_BASE,
    mark: p::SOUTHEASTASIAN_MARK,
};
const MONGOLIAN: Script = Script {
    base: p::MONGOLIAN_BASE,
    mark: p::MONGOLIAN_MARK,
};
const KHMER: Script = Script {
    base: p::KHMER_BASE,
    mark: p::KHMER_MARK,
};
const EGYPTIAN: Script = Script {
    base: p::EGYPTIAN_BASE,
    mark: p::EGYPTIAN_MARK,
};
const DUPLOYAN: Script = Script {
    base: p::DUPLOYAN_BASE,
    mark: p::DUPLOYAN_MARK,
};

pub fn cleaned(text: &str) -> String {
    if text
        .chars()
        .all(|c| c == '\t' || c == '\n' || (' '..='~').contains(&c))
    {
        return text.into();
    }
    Cleaner {
        scalars: text.chars().collect(),
        recent: Vec::new(),
        contextual: false,
        line_start: 0,
        line_rtl: None,
        line_arabic: None,
    }
    .run()
}
struct Cleaner {
    scalars: Vec<char>,
    recent: Vec<char>,
    contextual: bool,
    line_start: usize,
    line_rtl: Option<bool>,
    line_arabic: Option<bool>,
}
impl Cleaner {
    fn run(mut self) -> String {
        let mut output = String::new();
        let mut index = 0;
        while index < self.scalars.len() {
            let scalar = self.scalars[index];
            index += 1;
            let next = self.scalars.get(index).copied();
            if !candidate(scalar) {
                if scalar == '\n' {
                    self.start_line(index);
                }
                output.push(scalar);
                self.remember(scalar, false);
                if scalar == '\u{1F3F4}' {
                    for name in ["gbeng", "gbsct", "gbwls"] {
                        let tags: Vec<_> = name
                            .chars()
                            .map(|c| char::from_u32(c as u32 + 0xE0000).unwrap())
                            .chain(['\u{E007F}'])
                            .collect();
                        if self.scalars[index..].starts_with(&tags) {
                            for tag in &tags {
                                output.push(*tag);
                                self.remember(*tag, true);
                            }
                            index += tags.len();
                            break;
                        }
                    }
                }
            } else if line_break(scalar) {
                if scalar != '\r' || next != Some('\n') {
                    output.push('\n');
                    self.remember('\n', false);
                    self.start_line(index);
                }
            } else if self.keeps(scalar, next) {
                output.push(scalar);
                self.remember(scalar, true);
            }
        }
        output
    }
    fn start_line(&mut self, index: usize) {
        self.line_start = index;
        self.line_rtl = None;
        self.line_arabic = None;
    }
    fn remember(&mut self, c: char, contextual: bool) {
        self.recent.push(c);
        if self.recent.len() > 16 {
            self.recent.remove(0);
        }
        self.contextual = contextual;
    }
    fn last_is(&self, set: Set) -> bool {
        !self.contextual && contains(set, self.recent.last().copied())
    }
    fn follows(&self, script: Script, any_base: bool) -> bool {
        if self.contextual {
            return false;
        }
        for c in self.recent.iter().rev().copied() {
            if contains(p::MARK, Some(c)) {
                if !contains(script.mark, Some(c)) {
                    return false;
                }
                continue;
            }
            return contains(script.base, Some(c)) && (any_base || contains(p::LETTER, Some(c)));
        }
        false
    }
    fn line_has(&self, set: Set) -> bool {
        self.scalars[self.line_start..]
            .iter()
            .copied()
            .take_while(|c| !line_break(*c))
            .any(|c| rtl_block(c) && contains(p::LETTER, Some(c)) && contains(set, Some(c)))
    }
    fn continues_direction(&self, next: Option<char>) -> bool {
        let Some(next) = next else {
            return false;
        };
        let letter = contains(p::LETTER, Some(next));
        if !letter && !contains(p::DIGIT, Some(next)) {
            return false;
        }
        for c in self.recent.iter().rev().copied() {
            if candidate(c) {
                return true;
            }
            if contains(p::MARK, Some(c)) {
                continue;
            }
            return if letter {
                contains(p::LETTER, Some(c)) && rtl(c) == rtl(next)
            } else {
                contains(p::DIGIT, Some(c))
            };
        }
        self.recent.len() >= 16
    }
    fn emoji_before_modifiers(&self) -> Option<char> {
        for (skipped, c) in self.recent.iter().rev().copied().enumerate() {
            if skipped >= 2 || !(c == '\u{FE0F}' || ('\u{1F3FB}'..='\u{1F3FF}').contains(&c)) {
                return Some(c);
            }
        }
        None
    }
    fn keeps(&mut self, c: char, next: Option<char>) -> bool {
        let last = self.recent.last().copied();
        match c as u32 {
            0x200C => {
                self.follows(NON_JOINER, false)
                    || last.is_some()
                        && !self.contextual
                        && letter(next, NON_JOINER)
                        && !contains(p::WHITESPACE, last)
                        && !contains(p::MARK, last)
            }
            0x200D => {
                if self.follows(JOINER, false) {
                    return true;
                }
                if last.is_some_and(|c| c != '\u{200D}')
                    && contains(p::PICTOGRAPHIC, self.emoji_before_modifiers())
                    && kept(next, p::PICTOGRAPHIC)
                {
                    return true;
                }
                last.is_some()
                    && !self.contextual
                    && letter(next, CURSIVE)
                    && !contains(p::LETTER, last)
                    && !contains(p::DIGIT, last)
                    && !contains(p::MARK, last)
            }
            0x200B => {
                let southeast = kept(next, SOUTHEAST.base) && !contains(p::MARK, next);
                self.follows(SOUTHEAST, true) && (southeast || kept(next, p::ASCIIDIGIT))
                    || southeast && self.last_is(p::ASCIIDIGIT)
            }
            0xFE0E | 0xFE0F => {
                !self.contextual
                    && last.is_some_and(|c| {
                        c as u32 >= 0xA9 && contains(p::EMOJI, Some(c))
                            || "0123456789#*".contains(c) && next == Some('\u{20E3}')
                    })
            }
            0xFE00..=0xFE02 => {
                !self.contextual
                    && last.is_some_and(|last| {
                        contains(EGYPTIAN.base, Some(last))
                            || c == '\u{FE00}'
                                && ((0x2200..=0x2AFF).contains(&(last as u32))
                                    && contains(p::MATHSYMBOL, Some(last))
                                    || contains(p::VARIATIONONESCRIPTS, Some(last)))
                    })
            }
            0x200E | 0x200F | 0x061C => {
                let line_fits = if c == '\u{061C}' {
                    let found = self
                        .line_arabic
                        .unwrap_or_else(|| self.line_has(p::ARABICLETTERMARKSCRIPTS));
                    self.line_arabic = Some(found);
                    found
                } else {
                    let found = self
                        .line_rtl
                        .unwrap_or_else(|| self.line_has(p::RIGHTTOLEFT));
                    self.line_rtl = Some(found);
                    found
                };
                line_fits
                    && !self.contextual
                    && next.is_none_or(|c| {
                        line_break(c) || !candidate(c) && !contains(p::MARK, Some(c))
                    })
                    && !self.continues_direction(next)
            }
            0x034F => self.last_is(p::MARK) || !self.contextual && kept(next, p::MARK),
            0x17B4 | 0x17B5 => self.follows(KHMER, false),
            0x180B..=0x180F => {
                self.follows(MONGOLIAN, false) || !self.contextual && letter(next, MONGOLIAN)
            }
            0x1107F => self.last_is(p::BRAHMI) && kept(next, p::BRAHMI),
            0x13430..=0x1343F => {
                self.follows(EGYPTIAN, false) || !self.contextual && letter(next, EGYPTIAN)
            }
            0x1BCA0..=0x1BCA3 => {
                self.follows(DUPLOYAN, false) || !self.contextual && letter(next, DUPLOYAN)
            }
            _ => false,
        }
    }
}
fn kept(c: Option<char>, set: Set) -> bool {
    c.is_some_and(|c| !candidate(c) && contains(set, Some(c)))
}
fn letter(c: Option<char>, script: Script) -> bool {
    kept(c, script.base) && contains(p::LETTER, c)
}
fn candidate(c: char) -> bool {
    matches!(c as u32, 0x00..=0x08 | 0x0B..=0x1F | 0x7F..=0x9F | 0xAD | 0x34F | 0x61C | 0x115F | 0x1160 | 0x17B4 | 0x17B5 | 0x180B..=0x180F | 0x200B..=0x200F | 0x2028..=0x202E | 0x2060..=0x206F | 0x3164 | 0xFE00..=0xFE0F | 0xFEFF | 0xFFA0 | 0xFFF0..=0xFFFB | 0x1107F | 0x13430..=0x1343F | 0x16FE4 | 0x1BCA0..=0x1BCA3 | 0x1D173..=0x1D17A | 0xE0000..=0xE0FFF)
}
fn line_break(c: char) -> bool {
    matches!(c as u32, 0x0A..=0x0D | 0x85 | 0x2028 | 0x2029)
}
fn rtl_block(c: char) -> bool {
    matches!(c as u32, 0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF | 0x10800..=0x10FFF | 0x1E800..=0x1EFFF)
}
fn rtl(c: char) -> bool {
    c == '\u{0640}' || contains(p::RIGHTTOLEFT, Some(c))
}
