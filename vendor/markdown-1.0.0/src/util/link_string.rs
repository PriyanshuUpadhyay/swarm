//! Compile an owned linked String range without changing source points.

use crate::event::{Event, Kind, Name};
use crate::util::{
    character_reference::{decode_named, destination_numeric},
    slice::{Position, Slice},
};
use alloc::string::String;

/// Fold resolved pieces; the caller supplies the complete owned event range.
pub(crate) fn compile(
    bytes: &[u8],
    events: &[Event],
    from: usize,
    to_exclusive: usize,
    line_start_space: bool,
) -> String {
    let mut value = String::new();
    let mut marker = b'&';
    let mut after_line_ending = false;
    for index in from..to_exclusive {
        let event = &events[index];
        if event.kind != Kind::Exit {
            continue;
        }
        match event.name {
            Name::CharacterReferenceMarker => marker = b'&',
            Name::CharacterReferenceMarkerNumeric => marker = b'#',
            Name::CharacterReferenceMarkerHexadecimal => marker = b'x',
            Name::LineEnding => value.push('\n'),
            Name::Data
            | Name::CharacterEscapeValue
            | Name::CharacterReferenceValue
            | Name::SpaceOrTab => {
                let slice = Slice::from_position(bytes, &Position::from_exit_event(events, index));
                if event.name == Name::CharacterReferenceValue {
                    let decoded = if marker == b'&' {
                        decode_named(slice.as_str(), true)
                    } else {
                        destination_numeric(slice.as_str(), marker)
                    };
                    if let Some(decoded) = decoded {
                        value.push_str(&decoded);
                    }
                } else if event.name != Name::SpaceOrTab || line_start_space || !after_line_ending {
                    value.push_str(&slice.serialize());
                }
            }
            _ => {}
        }
        after_line_ending = event.name == Name::LineEnding;
    }
    value
}
