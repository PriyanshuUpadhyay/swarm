//! mdurl 2.1.0 parse(url, true), format, and default encode, ported to Rust.
//!
//! See ../../NOTICES.md for the mdurl and Joyent notices.

use super::es_space;

#[derive(Default)]
pub(super) struct Url {
    pub(super) protocol: String,
    slashes: bool,
    pub(super) auth: String,
    port: String,
    pub(super) hostname: String,
    pub(super) pathname: String,
    pub(super) search: String,
    pub(super) hash: String,
}

fn slashed(protocol: &str) -> bool {
    matches!(protocol, "http:" | "https:" | "ftp:" | "gopher:" | "file:")
}

fn host_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '+' | '_' | '-')
}

fn line_terminator(ch: char) -> bool {
    matches!(ch, '\r' | '\n' | '\u{2028}' | '\u{2029}')
}

/// The JS regex has no multiline flag, so `$` requires the actual end.
fn part_matches(part: &str) -> bool {
    part.len() <= 63 && part.chars().all(host_char)
}

fn part_start(part: &str) -> Option<(&str, &str)> {
    let end = part
        .char_indices()
        .take(63)
        .take_while(|&(_, ch)| host_char(ch))
        .map(|(i, ch)| i + ch.len_utf8())
        .last()
        .unwrap_or(0);
    let tail = &part[end..];
    if tail.chars().any(line_terminator) {
        None
    } else {
        Some((&part[..end], tail))
    }
}

pub(super) fn parse(input: &str) -> Url {
    // slashesDenoteHost=true disables mdurl's simplePath fast path.
    let mut rest = input.trim_matches(es_space).to_owned();
    let mut url = Url::default();
    if let Some(colon) = rest.find(':')
        && colon > 0
        && rest[..colon]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".+-".contains(&b))
    {
        url.protocol = rest[..=colon].into();
        rest = rest[colon + 1..].into();
    }
    // Pinned tables are case sensitive and use the ORIGINAL protocol spelling.
    let hostless = url.protocol == "javascript:";
    let slashes = rest.starts_with("//");
    if slashes && !hostless {
        rest = rest[2..].into();
        url.slashes = true;
    }
    if !hostless && (slashes || (!url.protocol.is_empty() && !slashed(&url.protocol))) {
        let host_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        if let Some(at) = rest[..host_end].rfind('@') {
            url.auth = rest[..at].into();
            rest = rest[at + 1..].into();
        }
        let mut end = rest
            .find([
                '%', '/', '?', ';', '#', '\'', '{', '}', '|', '\\', '^', '`', '<', '>', '"', ' ',
                '\r', '\n', '\t',
            ])
            .unwrap_or(rest.len());
        if end > 0 && rest.as_bytes()[end - 1] == b':' {
            end -= 1;
        }
        let host = rest[..end].to_owned();
        rest = rest[end..].into();
        let mut hostname = host.as_str();
        if let Some(colon) = host.rfind(':')
            && host[colon + 1..].bytes().all(|b| b.is_ascii_digit())
        {
            url.port = host[colon + 1..].into();
            hostname = &host[..colon];
        }
        url.hostname = hostname.into();
        let ipv6 = hostname.starts_with('[') && hostname.ends_with(']');
        if !ipv6 {
            let parts: Vec<&str> = hostname.split('.').collect();
            for (i, part) in parts.iter().enumerate() {
                if part.is_empty() || part_matches(part) {
                    continue;
                }
                // JS substitutes every non-ASCII UTF16 UNIT, preserving length.
                let temporary: String = part
                    .encode_utf16()
                    .map(|c| if c > 127 { 'x' } else { c as u8 as char })
                    .collect();
                if !part_matches(&temporary) {
                    let mut valid = parts[..i].to_vec();
                    let mut not_host = parts[i + 1..].to_vec();
                    if let Some((prefix, tail)) = part_start(part) {
                        valid.push(prefix);
                        not_host.insert(0, tail);
                    }
                    if !not_host.is_empty() {
                        rest = not_host.join(".") + &rest;
                    }
                    url.hostname = valid.join(".");
                    break;
                }
            }
        }
        if url.hostname.encode_utf16().count() > 255 {
            url.hostname.clear();
        }
        if ipv6 {
            // JS substr(1, length-2) also returns empty after the length guard.
            url.hostname = if url.hostname.len() >= 2 {
                url.hostname[1..url.hostname.len() - 1].into()
            } else {
                String::new()
            };
        }
    }
    if let Some(hash) = rest.find('#') {
        url.hash = rest[hash..].into();
        rest.truncate(hash);
    }
    if let Some(qm) = rest.find('?') {
        url.search = rest[qm..].into();
        rest.truncate(qm);
    }
    url.pathname = rest;
    // mdurl's final lowerProto pathname='' branch is already represented here.
    url
}

pub(super) fn format(url: &Url) -> String {
    let mut result = url.protocol.clone();
    if url.slashes {
        result.push_str("//");
    }
    if !url.auth.is_empty() {
        result.push_str(&url.auth);
        result.push('@');
    }
    if url.hostname.contains(':') {
        result.push('[');
        result.push_str(&url.hostname);
        result.push(']');
    } else {
        result.push_str(&url.hostname);
    }
    if !url.port.is_empty() {
        result.push(':');
        result.push_str(&url.port);
    }
    result.push_str(&url.pathname);
    result.push_str(&url.search);
    result.push_str(&url.hash);
    result
}

pub(super) fn encode(input: &str) -> String {
    const SAFE: &[u8] = b";/?:@&=+$,-_.!~*'()#";
    const HEX: &[u8] = b"0123456789ABCDEF";
    let bytes = input.as_bytes();
    let mut output = String::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%'
            && i + 2 < bytes.len()
            && bytes[i + 1].is_ascii_hexdigit()
            && bytes[i + 2].is_ascii_hexdigit()
        {
            output.push_str(&input[i..i + 3]);
            i += 3;
        } else if b.is_ascii_alphanumeric() || SAFE.contains(&b) {
            output.push(b as char);
            i += 1;
        } else {
            // For valid Rust strings, JS encodeURIComponent is UTF8 byte encoding.
            output.push('%');
            output.push(HEX[(b >> 4) as usize] as char);
            output.push(HEX[(b & 15) as usize] as char);
            i += 1;
        }
    }
    output
}
