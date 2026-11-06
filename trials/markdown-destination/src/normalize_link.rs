//! Pinned MarkdownIt 15.0.2 normalizeLink and the application's HTTP(S) gate.
//!
//! The pure URL routines retain mdurl 2.1.0 and punycode.js 2.3.1 semantics.
//! Their notices and source pins are in ../NOTICES.md.

mod mdurl;
mod punycode;

/// Apply OLD preprocessing only to the private href copy.
pub(crate) fn normalize(value: &str) -> String {
    let value = value.replace('\0', "\u{fffd}");
    let mut url = mdurl::parse(&value);
    if !url.hostname.is_empty()
        && matches!(url.protocol.as_str(), "" | "http:" | "https:" | "mailto:")
    {
        // The JS glue keeps the original host when toASCII throws.
        if let Some(host) = punycode::to_ascii(&url.hostname) {
            url.hostname = host;
        }
    }
    url.auth = mdurl::encode(&url.auth);
    url.hostname = mdurl::encode(&url.hostname);
    url.pathname = mdurl::encode(&url.pathname);
    url.search = mdurl::encode(&url.search);
    url.hash = mdurl::encode(&url.hash);
    mdurl::format(&url)
}

pub(crate) fn validate(value: &str) -> bool {
    let bytes = value.trim_matches(es_space).as_bytes();
    bytes
        .get(..7)
        .is_some_and(|p| p.eq_ignore_ascii_case(b"http://"))
        || bytes
            .get(..8)
            .is_some_and(|p| p.eq_ignore_ascii_case(b"https://"))
}

/// ECMAScript 2025 WhiteSpace + LineTerminator, with Unicode 16 Zs.
/// U+0085 and U+180E are deliberately absent.
fn es_space(ch: char) -> bool {
    matches!(ch, '\u{9}'..='\u{d}' | ' ' | '\u{a0}' | '\u{1680}'
        | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}'
        | '\u{205f}' | '\u{3000}' | '\u{feff}')
}
