//! Turn bytes of markdown into events.

use crate::event::{Event, Point};
use crate::message;
use crate::state::{Name as StateName, State};
use crate::subtokenize::subtokenize;
use crate::tokenizer::Tokenizer;
use crate::util::{location::Location, normalize_identifier::fold_es_whitespace};
use crate::ParseOptions;
use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::String,
    vec,
    vec::Vec,
};

/// Info needed, in all content types, when parsing markdown.
///
/// Importantly, this contains a set of known definitions.
/// It also references the input value as bytes (`u8`).
#[derive(Debug)]
pub struct ParseState<'a> {
    /// Configuration.
    pub location: Option<Location>,
    /// Configuration.
    pub options: &'a ParseOptions,
    /// Whether the caller is the mdast entry point.
    pub mdast: bool,
    /// Owned title values keyed by their title-string Enter's source byte index.
    pub link_titles: BTreeMap<usize, String>,
    /// List of chars.
    pub bytes: &'a [u8],
    /// Set of defined definition identifiers.
    pub definitions: Vec<String>,
    /// Nonempty folded definition identifiers for the private linked value extension.
    pub es_definition_folds: BTreeSet<String>,
    /// Set of defined GFM footnote definition identifiers.
    pub gfm_footnote_definitions: Vec<String>,
}

impl ParseState<'_> {
    /// Select the private linked value extension for a hooked mdast caller.
    pub(crate) fn link_compat(&self) -> bool {
        self.mdast
            && (self.options.normalize_link.is_some() || self.options.validate_link.is_some())
    }
}

/// Turn a string of markdown into events.
///
/// Passes the bytes back so the compiler can access the source.
pub fn parse<'a>(
    value: &'a str,
    options: &'a ParseOptions,
    mdast: bool,
) -> Result<(Vec<Event>, ParseState<'a>), message::Message> {
    let bytes = value.as_bytes();

    let mut parse_state = ParseState {
        options,
        mdast,
        link_titles: BTreeMap::new(),
        bytes,
        location: if options.mdx_esm_parse.is_some() || options.mdx_expression_parse.is_some() {
            Some(Location::new(bytes))
        } else {
            None
        },
        definitions: vec![],
        es_definition_folds: BTreeSet::new(),
        gfm_footnote_definitions: vec![],
    };

    let start = Point {
        line: 1,
        column: 1,
        index: 0,
        vs: 0,
    };
    let mut tokenizer = Tokenizer::new(start, &parse_state);

    let state = tokenizer.push(
        (0, 0),
        (parse_state.bytes.len(), 0),
        State::Next(StateName::DocumentStart),
    );
    let mut result = tokenizer.flush(state, true)?;
    let mut events = tokenizer.events;

    loop {
        if parse_state.link_compat() && !options.constructs.gfm_label_start_footnote {
            for key in &result.definitions {
                let folded = fold_es_whitespace(key);
                if !folded.is_empty() {
                    parse_state.es_definition_folds.insert(folded);
                }
            }
        }

        let fn_defs = &mut parse_state.gfm_footnote_definitions;
        let defs = &mut parse_state.definitions;
        fn_defs.append(&mut result.gfm_footnote_definitions);
        defs.append(&mut result.definitions);
        parse_state.link_titles.append(&mut result.link_titles);

        if result.done {
            return Ok((events, parse_state));
        }

        result = subtokenize(&mut events, &parse_state, None)?;
    }
}
