//! Borrowed JSON fields keep unknown and deeply nested source data unexpanded.

use super::{SnapshotError, Work};
use serde::de::{MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;

// OLD treats negative zero as a float even without a fraction or exponent.
pub(super) fn integer_formatted(text: &str) -> bool {
    text != "-0" && !text.contains(['.', 'e', 'E'])
}

pub(super) struct Object<'a>(Vec<(String, &'a RawValue)>);

impl<'de> Deserialize<'de> for Object<'de> {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        struct Fields;
        impl<'de> Visitor<'de> for Fields {
            type Value = Object<'de>;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a JSON object")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
                let mut fields: Vec<(String, &'de RawValue)> = Vec::new();
                while let Some((key, value)) = map.next_entry::<String, &'de RawValue>()? {
                    if fields.iter().any(|(name, _)| *name == key) {
                        return Err(serde::de::Error::custom("duplicate JSON field"));
                    }
                    fields.push((key, value));
                }
                Ok(Object(fields))
            }
        }
        de.deserialize_map(Fields)
    }
}

impl<'a> Object<'a> {
    pub(super) fn parse(raw: &'a RawValue) -> Option<Self> {
        serde_json::from_str(raw.get()).ok()
    }
    pub(super) fn get(&self, key: &str) -> Option<&'a RawValue> {
        self.0
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, raw)| *raw)
    }
    pub(super) fn text(&self, key: &str) -> String {
        self.optional_text(key).unwrap_or_default()
    }
    pub(super) fn optional_text(&self, key: &str) -> Option<String> {
        serde_json::from_str(self.get(key)?.get()).ok()
    }
    pub(super) fn object(&self, key: &str) -> Option<Self> {
        Self::parse(self.get(key)?)
    }
    pub(super) fn array(&self, key: &str) -> Option<Vec<&'a RawValue>> {
        array(self.get(key)?)
    }
    pub(super) fn integer(&self, key: &str) -> Option<i64> {
        let raw = self.get(key)?.get();
        if !integer_formatted(raw) {
            return None;
        }
        raw.parse().ok()
    }
    pub(super) fn token(&self, key: &str) -> Option<i64> {
        self.integer(key).filter(|value| *value >= 0)
    }
    pub(super) fn nonnegative_number(&self, key: &str) -> Option<f64> {
        let raw = self.get(key)?.get();
        let value = if !integer_formatted(raw) {
            raw.parse::<f64>().ok()?
        } else {
            raw.parse::<i64>().ok()? as f64
        };
        (value.is_finite() && value >= 0.0).then_some(value)
    }
    pub(super) fn boolean(&self, key: &str) -> Option<bool> {
        serde_json::from_str(self.get(key)?.get()).ok()
    }
    pub(super) fn fields(&self) -> impl Iterator<Item = (&str, &'a RawValue)> {
        self.0.iter().map(|(key, value)| (key.as_str(), *value))
    }
}

pub(super) fn array(raw: &RawValue) -> Option<Vec<&RawValue>> {
    serde_json::from_str(raw.get()).ok()
}

// RawValue validates structure and number syntax, including OLD's arbitrary precision numbers.
// Visit strings separately because its skip does not reject invalid Unicode escapes.
pub(super) fn valid(raw: &RawValue, work: &mut dyn Work) -> Result<bool, SnapshotError> {
    let mut stack = vec![raw];
    while let Some(raw) = stack.pop() {
        work.check()?;
        match raw.get().as_bytes().first() {
            Some(b'{') => {
                let Some(obj) = Object::parse(raw) else {
                    return Ok(false);
                };
                stack.extend(obj.fields().map(|(_, raw)| raw));
            }
            Some(b'[') => {
                let Some(items) = array(raw) else {
                    return Ok(false);
                };
                stack.extend(items);
            }
            Some(b'"') if serde_json::from_str::<String>(raw.get()).is_err() => return Ok(false),
            _ => (),
        }
    }
    Ok(true)
}
