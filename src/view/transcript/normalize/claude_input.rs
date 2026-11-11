//! Bounded tool inputs keep OLD's i64, finite f64, and raw number-string decisions.

use super::claude_json::{self, Object};
use super::{SnapshotError, Work};
use serde::ser::{SerializeMap, SerializeSeq};
use serde::{Serialize, Serializer};
use serde_json::{Value, value::RawValue};

#[derive(Debug)]
pub(super) enum Input {
    Scalar(Value),
    Number(Box<RawValue>),
    Array(Vec<Input>),
    Object(Vec<(String, Input)>),
}

impl Serialize for Input {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Scalar(Value::Number(number)) if number.is_f64() => {
                let raw = RawValue::from_string(
                    number.as_f64().expect("finite parsed float").to_string(),
                )
                .map_err(<S::Error as serde::ser::Error>::custom)?;
                raw.serialize(serializer)
            }
            Self::Scalar(value) => value.serialize(serializer),
            Self::Number(raw) => raw.serialize(serializer),
            Self::Array(items) => {
                let mut seq = serializer.serialize_seq(Some(items.len()))?;
                for item in items {
                    seq.serialize_element(item)?;
                }
                seq.end()
            }
            Self::Object(fields) => {
                let mut map = serializer.serialize_map(Some(fields.len()))?;
                for (key, value) in fields {
                    map.serialize_entry(key, value)?;
                }
                map.end()
            }
        }
    }
}

pub(super) fn parse(
    raw: &RawValue,
    remaining: usize,
    work: &mut dyn Work,
) -> Result<Option<Input>, SnapshotError> {
    work.check()?;
    let value = match raw.get().as_bytes().first() {
        Some(b'[' | b'{') if remaining == 0 => return Ok(None),
        Some(b'[') => {
            let Some(items) = claude_json::array(raw) else {
                return Ok(None);
            };
            let mut values = Vec::with_capacity(items.len());
            for item in items {
                let Some(value) = parse(item, remaining - 1, work)? else {
                    return Ok(None);
                };
                values.push(value);
            }
            Input::Array(values)
        }
        Some(b'{') => {
            let Some(object) = Object::parse(raw) else {
                return Ok(None);
            };
            let mut fields = Vec::new();
            for (key, raw) in object.fields() {
                let Some(value) = parse(raw, remaining - 1, work)? else {
                    return Ok(None);
                };
                fields.push((key.to_owned(), value));
            }
            Input::Object(fields)
        }
        Some(b'-' | b'0'..=b'9') => {
            let text = raw.get();
            if !claude_json::integer_formatted(text) {
                match text
                    .parse::<f64>()
                    .ok()
                    .and_then(serde_json::Number::from_f64)
                {
                    Some(number) => Input::Scalar(Value::Number(number)),
                    None => Input::Number(raw.to_owned()),
                }
            } else {
                match text.parse::<i64>() {
                    Ok(number) => Input::Scalar(number.into()),
                    Err(_) => Input::Number(raw.to_owned()),
                }
            }
        }
        _ => {
            // Containers were handled above; this decoder sees one already-validated scalar.
            let Ok(value) = serde_json::from_str(raw.get()) else {
                return Ok(None);
            };
            Input::Scalar(value)
        }
    };
    Ok(Some(value))
}
