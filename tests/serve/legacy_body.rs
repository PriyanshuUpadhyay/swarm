// E2-2: the frozen body_wire tests cover the additive body choice.
fn without_body(mut value: serde_json::Value) -> serde_json::Value {
    fn strip(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                let page = map.contains_key("page") && map.contains_key("rows");
                let frame = map.contains_key("key") && map.contains_key("upsert");
                let queue =
                    map.contains_key("mode") && map.contains_key("id") && map.contains_key("text");
                if map.contains_key("display") && (page || frame || queue) {
                    map.shift_remove("body");
                }
                for child in map.values_mut() {
                    strip(child);
                }
            }
            serde_json::Value::Array(items) => items.iter_mut().for_each(strip),
            _ => {}
        }
    }
    strip(&mut value);
    value
}
