//! Destination occurs in [definition][] and [label end][label_end].
//!
//! ## Grammar
//!
//! Destination forms with the following BNF
//! (<small>see [construct][crate::construct] for character groups</small>):
//!
//! ```bnf
//! destination ::= destination_enclosed | destination_raw
//!
//! destination_enclosed ::= '<' *(destination_enclosed_byte | destination_enclosed_escape) '>'
//! destination_enclosed_byte ::= line - '<' - '\\' - '>'
//! destination_enclosed_escape ::= '\\' ['<' | '\\' | '>']
//!
//! destination_raw ::= 1*(destination_raw_byte | destination_raw_escape)
//! ; Restriction: unbalanced `)` characters are not allowed.
//! destination_raw_byte ::= text - '\\' - ascii_control
//! destination_raw_escape ::= '\\' ['(' | ')' | '\\']
//! ```
//!
//! Balanced parens allowed in raw destinations.
//! They are counted with a counter that starts at `0`, and is incremented
//! every time `(` occurs and decremented every time `)` occurs.
//! If `)` is found when the counter is `0`, the destination closes immediately
//! before it.
//! Escaped parens do not count in balancing.
//!
//! The destination is interpreted as the [string][] content type.
//! That means that [character escapes][character_escape] and
//! [character references][character_reference] are allowed.
//!
//! The grammar for enclosed destinations (`<x>`) prohibits the use of `<`,
//! `>`, and line endings to form URLs.
//! The angle brackets can be encoded as a character reference, character
//! escape, or percent encoding:
//!
//! * `<` as `&lt;`, `\<`, or `%3c`
//! * `>` as `&gt;`, `\>`, or `%3e`
//!
//! The grammar for raw destinations (`x`) prohibits space (` `) and all
//! [ASCII control][u8::is_ascii_control] characters, which thus must be
//! encoded.
//! Unbalanced parens can be encoded as a character reference, character escape,
//! or percent encoding:
//!
//! * `(` as `&lpar;`, `\(`, or `%28`
//! * `)` as `&rpar;`, `\)`, or `%29`
//!
//! There are several cases where incorrect encoding of URLs would, in other
//! languages, result in a parse error.
//! In markdown, there are no errors, and URLs are normalized.
//! In addition, unicode characters are percent encoded
//! ([`sanitize_uri`][sanitize_uri]).
//! For example:
//!
//! ```markdown
//! [x]
//!
//! [x]: <https://a👍b%>
//! ```
//!
//! Yields:
//!
//! ```html
//! <p><a href="https://a%F0%9F%91%8Db%25">x</a></p>
//! ```
//!
//! ## Recommendation
//!
//! It is recommended to use the enclosed variant of destinations, as it allows
//! the most characters, including arbitrary parens, in URLs.
//!
//! ## References
//!
//! * [`micromark-factory-destination/index.js` in `micromark`](https://github.com/micromark/micromark/blob/main/packages/micromark-factory-destination/dev/index.js)
//!
//! [definition]: crate::construct::definition
//! [string]: crate::construct::string
//! [character_escape]: crate::construct::character_escape
//! [character_reference]: crate::construct::character_reference
//! [label_end]: crate::construct::label_end
//! [sanitize_uri]: crate::util::sanitize_uri

use crate::construct::partial_space_or_tab::{
    space_or_tab_with_options, Options as SpaceOrTabOptions,
};
use crate::event::{Content, Kind, Link, Name};
use crate::state::{Name as StateName, State};
use crate::subtokenize::link;
use crate::tokenizer::Tokenizer;
use crate::util::link_string;
use alloc::string::String;

/// Start of destination.
///
/// ```markdown
/// > | <aa>
///     ^
/// > | aa
///     ^
/// ```
pub fn start(tokenizer: &mut Tokenizer) -> State {
    tokenizer.tokenize_state.destination_definition_eol = false;
    match tokenizer.current {
        Some(b'<') => {
            tokenizer.enter(tokenizer.tokenize_state.token_1.clone());
            tokenizer.enter(tokenizer.tokenize_state.token_2.clone());
            tokenizer.enter(tokenizer.tokenize_state.token_3.clone());
            tokenizer.consume();
            tokenizer.exit(tokenizer.tokenize_state.token_3.clone());
            State::Next(StateName::DestinationEnclosedBefore)
        }
        // ASCII control, space, closing paren, but *not* `\0`.
        None | Some(0x01..=0x1F | b' ' | b')' | 0x7F) => State::Nok,
        Some(_) => {
            tokenizer.enter(tokenizer.tokenize_state.token_1.clone());
            tokenizer.enter(tokenizer.tokenize_state.token_4.clone());
            tokenizer.enter(tokenizer.tokenize_state.token_5.clone());
            tokenizer.enter_link(
                Name::Data,
                Link {
                    previous: None,
                    next: None,
                    content: Content::String,
                },
            );
            State::Retry(StateName::DestinationRaw)
        }
    }
}

/// After `<`, at an enclosed destination.
///
/// ```markdown
/// > | <aa>
///      ^
/// ```
pub fn enclosed_before(tokenizer: &mut Tokenizer) -> State {
    if let Some(b'>') = tokenizer.current {
        tokenizer.enter(tokenizer.tokenize_state.token_3.clone());
        tokenizer.consume();
        tokenizer.exit(tokenizer.tokenize_state.token_3.clone());
        tokenizer.exit(tokenizer.tokenize_state.token_2.clone());
        tokenizer.exit(tokenizer.tokenize_state.token_1.clone());
        destination_policy(tokenizer)
    } else {
        tokenizer.enter(tokenizer.tokenize_state.token_5.clone());
        tokenizer.enter_link(
            Name::Data,
            Link {
                previous: None,
                next: None,
                content: Content::String,
            },
        );
        State::Retry(StateName::DestinationEnclosed)
    }
}

/// In enclosed destination.
///
/// ```markdown
/// > | <aa>
///      ^
/// ```
pub fn enclosed(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None | Some(b'\n' | b'<') => State::Nok,
        Some(b'>') => {
            tokenizer.exit(Name::Data);
            tokenizer.exit(tokenizer.tokenize_state.token_5.clone());
            State::Retry(StateName::DestinationEnclosedBefore)
        }
        Some(b'\\') => {
            tokenizer.consume();
            State::Next(StateName::DestinationEnclosedEscape)
        }
        _ => {
            tokenizer.consume();
            State::Next(StateName::DestinationEnclosed)
        }
    }
}

/// After `\`, at a special character.
///
/// ```markdown
/// > | <a\*a>
///        ^
/// ```
pub fn enclosed_escape(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        Some(b'\n')
            if tokenizer.parse_state.link_compat()
                && tokenizer.tokenize_state.token_1 == Name::ResourceDestination =>
        {
            tokenizer.exit(Name::Data);
            continuation(tokenizer, Name::LineEnding);
            tokenizer.consume();
            tokenizer.exit(Name::LineEnding);
            State::Next(StateName::DestinationEnclosedAfterEol)
        }
        Some(b'<' | b'>' | b'\\') => {
            tokenizer.consume();
            State::Next(StateName::DestinationEnclosed)
        }
        _ => State::Retry(StateName::DestinationEnclosed),
    }
}

/// Open a new enclosed piece after the tokenizer applied its container skip.
pub fn enclosed_after_eol(tokenizer: &mut Tokenizer) -> State {
    match tokenizer.current {
        None | Some(b'\n' | b'<') => State::Nok,
        Some(b'>') => {
            tokenizer.exit(tokenizer.tokenize_state.token_5.clone());
            State::Retry(StateName::DestinationEnclosedBefore)
        }
        Some(b'\t' | b' ') => {
            tokenizer.attempt(
                State::Next(StateName::DestinationEnclosedAfterEol),
                State::Nok,
            );
            State::Retry(space_or_tab_with_options(
                tokenizer,
                SpaceOrTabOptions {
                    kind: Name::SpaceOrTab,
                    min: 1,
                    max: usize::MAX,
                    content: Some(Content::String),
                    connect: true,
                },
            ))
        }
        _ => {
            continuation(tokenizer, Name::Data);
            State::Retry(StateName::DestinationEnclosed)
        }
    }
}

/// Connect only canonical source pieces, never the container prefix between them.
fn continuation(tokenizer: &mut Tokenizer, name: Name) {
    tokenizer.enter_link(
        name,
        Link {
            previous: None,
            next: None,
            content: Content::String,
        },
    );
    let index = tokenizer.events.len() - 1;
    link(&mut tokenizer.events, index);
}

/// Close the raw owner after its last nonempty Data piece has closed.
fn raw_end(tokenizer: &mut Tokenizer) -> State {
    tokenizer.exit(tokenizer.tokenize_state.token_5.clone());
    tokenizer.exit(tokenizer.tokenize_state.token_4.clone());
    tokenizer.exit(tokenizer.tokenize_state.token_1.clone());
    tokenizer.tokenize_state.size = 0;
    destination_policy(tokenizer)
}

/// Resume raw grammar without creating empty Data at a closing delimiter.
pub fn raw_after_eol(tokenizer: &mut Tokenizer) -> State {
    if tokenizer.tokenize_state.size == 0
        && matches!(tokenizer.current, None | Some(b'\t' | b'\n' | b' ' | b')'))
    {
        raw_end(tokenizer)
    } else if matches!(tokenizer.current, None | Some(0x01..=0x1F | b' ' | 0x7F)) {
        tokenizer.tokenize_state.size = 0;
        State::Nok
    } else {
        continuation(tokenizer, Name::Data);
        State::Retry(StateName::DestinationRaw)
    }
}

/// In raw destination.
///
/// ```markdown
/// > | aa
///     ^
/// ```
pub fn raw(tokenizer: &mut Tokenizer) -> State {
    if tokenizer.tokenize_state.size == 0
        && matches!(tokenizer.current, None | Some(b'\t' | b'\n' | b' ' | b')'))
    {
        tokenizer.exit(Name::Data);
        raw_end(tokenizer)
    } else if tokenizer.tokenize_state.size < tokenizer.tokenize_state.size_b
        && tokenizer.current == Some(b'(')
    {
        tokenizer.consume();
        tokenizer.tokenize_state.size += 1;
        State::Next(StateName::DestinationRaw)
    } else if tokenizer.current == Some(b')') {
        tokenizer.consume();
        tokenizer.tokenize_state.size -= 1;
        State::Next(StateName::DestinationRaw)
    }
    // ASCII control (but *not* `\0`) and space and `(`.
    else if matches!(
        tokenizer.current,
        None | Some(0x01..=0x1F | b' ' | b'(' | 0x7F)
    ) {
        tokenizer.tokenize_state.size = 0;
        tokenizer.tokenize_state.destination_definition_eol = false;
        State::Nok
    } else if tokenizer.current == Some(b'\\') {
        tokenizer.consume();
        State::Next(StateName::DestinationRawEscape)
    } else {
        tokenizer.consume();
        State::Next(StateName::DestinationRaw)
    }
}

/// After `\`, at special character.
///
/// ```markdown
/// > | a\*a
///       ^
/// ```
pub fn raw_escape(tokenizer: &mut Tokenizer) -> State {
    // Finish the escaped physical TAB before returning to ordinary raw grammar.
    if tokenizer.point.vs > 0
        && (tokenizer.parse_state.options.normalize_link.is_some()
            || tokenizer.parse_state.options.validate_link.is_some())
    {
        tokenizer.consume();
        return State::Next(if tokenizer.point.vs > 0 {
            StateName::DestinationRawEscape
        } else {
            StateName::DestinationRaw
        });
    }

    match tokenizer.current {
        Some(b'\n')
            if tokenizer.parse_state.link_compat()
                && tokenizer.tokenize_state.token_1 == Name::ResourceDestination =>
        {
            tokenizer.exit(Name::Data);
            continuation(tokenizer, Name::LineEnding);
            tokenizer.consume();
            tokenizer.exit(Name::LineEnding);
            State::Next(StateName::DestinationRawAfterEol)
        }
        Some(b'\n')
            if tokenizer.parse_state.link_compat()
                && tokenizer.tokenize_state.token_1 == Name::DefinitionDestination =>
        {
            tokenizer.tokenize_state.destination_definition_eol = true;
            State::Retry(StateName::DestinationRaw)
        }
        Some(b'(' | b')' | b'\\') => {
            tokenizer.consume();
            State::Next(StateName::DestinationRaw)
        }
        Some(0x01..=0x09 | 0x0B..=0x0C | 0x0E..=0x1F | 0x7F)
            if tokenizer.parse_state.options.normalize_link.is_some()
                || tokenizer.parse_state.options.validate_link.is_some() =>
        {
            tokenizer.consume();
            State::Next(if tokenizer.point.vs > 0 {
                StateName::DestinationRawEscape
            } else {
                StateName::DestinationRaw
            })
        }
        _ => State::Retry(StateName::DestinationRaw),
    }
}

/// Apply the two hooks to an owning exit. The existing caller owns rollback.
pub(crate) fn retain_href(tokenizer: &mut Tokenizer, decoded: String) -> State {
    let options = &tokenizer.parse_state.options;
    if options.normalize_link.is_none() && options.validate_link.is_none() {
        return State::Ok;
    }
    let href = if let Some(normalize) = options.normalize_link {
        normalize(&decoded)
    } else {
        decoded
    };
    if matches!(options.validate_link, Some(validate) if !validate(&href)) {
        tokenizer.tokenize_state.destination_definition_eol = false;
        return State::Nok;
    }
    tokenizer
        .events
        .last_mut()
        .expect("owning destination exit")
        .normalized_href = Some(href.into_boxed_str());
    State::Ok
}

/// Decode this destination's complete String chain once, before admission.
fn destination_policy(tokenizer: &mut Tokenizer) -> State {
    let options = &tokenizer.parse_state.options;
    if options.normalize_link.is_none() && options.validate_link.is_none() {
        return State::Ok;
    }
    let exit = tokenizer.events.len() - 1;
    let enter = tokenizer.events[..exit]
        .iter()
        .rposition(|event| event.kind == Kind::Enter && event.name == tokenizer.events[exit].name)
        .expect("owning destination enter");
    let mut decoded = String::new();
    // Empty <> has no String chain.
    let span = tokenizer.events[enter + 1..exit]
        .iter()
        .enumerate()
        .find(|(_, event)| {
            event.kind == Kind::Enter
                && event.name == Name::Data
                && event.link.as_ref().map_or(false, |link| {
                    link.content == Content::String && link.previous.is_none()
                })
        });
    if let Some((index, event)) = span {
        let mut local = Tokenizer::new(event.point.clone(), tokenizer.parse_state);
        local.destination_string = true;
        let mut link_index = Some(enter + index + 1);
        let mut state = State::Next(StateName::StringStart);
        while let Some(index) = link_index {
            let event = &tokenizer.events[index];
            let link = event.link.as_ref().expect("owned String link");
            debug_assert!(index > enter && index + 1 < exit);
            if link.previous.is_some() {
                local.define_skip(event.point.clone());
            }
            let end = &tokenizer.events[index + 1].point;
            state = local.push(
                (event.point.index, event.point.vs),
                (end.index, end.vs),
                state,
            );
            link_index = link.next;
        }
        if let Err(error) = local.flush(state, true) {
            tokenizer.tokenize_state.destination_definition_eol = false;
            return State::Error(error);
        }
        decoded = link_string::compile(
            local.parse_state.bytes,
            &local.events,
            0,
            local.events.len(),
            true,
        );
    }
    if tokenizer.tokenize_state.destination_definition_eol {
        decoded.push('\n');
    }
    retain_href(tokenizer, decoded)
}
