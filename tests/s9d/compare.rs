include!("../serve/legacy_body.rs");
fn empty_client(key: &str) -> Value {
    json!({"rows":{},"display":{},"patch":{},"find":{},"images":{},"usage":null,"end_offset":0,
        "live":{"key":key,"text":null,"thinking":null,"display":null,"ended":false,"hide_text":false,"hide_thinking":false}})
}
fn row_ids(value: &Value, ids: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            if let Some(id) = object.get("id").and_then(Value::as_str) {
                ids.push(id.into());
            }
            for value in object.values() {
                row_ids(value, ids);
            }
        }
        Value::Array(array) => {
            for value in array {
                row_ids(value, ids);
            }
        }
        _ => {}
    }
}
// OLD live_delta.rs:11-23 owns row fields; reader_rows/delta.rs applies ids and usage changes.
fn apply_frames(state: &mut Value, frames: &[Value]) {
    for frame in frames {
        let event = frame["event"].as_str().unwrap();
        assert!(matches!(event, "rows" | "reset" | "live"));
        let data = &frame["data"];
        let key = data["key"].as_str().unwrap();
        let clients = state.as_object_mut().unwrap();
        let state = clients.entry(key).or_insert_with(|| empty_client(key));
        if event == "reset" {
            *state = empty_client(key);
            continue;
        }
        if event == "live" {
            state["live"] = data.clone();
            continue;
        }
        let upsert = data["upsert"].as_array().unwrap();
        let mut replace = data["remove"].as_array().unwrap().clone();
        replace.extend(upsert.iter().map(|row| row["id"].clone()));
        for id in replace {
            if let Some(old) = state["rows"]
                .as_object_mut()
                .unwrap()
                .remove(id.as_str().unwrap())
            {
                let mut ids = Vec::new();
                row_ids(&old, &mut ids);
                for field in ["display", "patch", "find", "images"] {
                    for id in &ids {
                        state[field].as_object_mut().unwrap().remove(id);
                    }
                }
            }
        }
        for row in upsert {
            let id = row["id"].as_str().unwrap();
            state["rows"]
                .as_object_mut()
                .unwrap()
                .insert(id.into(), row.clone());
        }
        for field in ["display", "patch", "find", "images"] {
            let map = data[field].as_object().unwrap();
            state[field].as_object_mut().unwrap().extend(map.clone());
        }
        if let Some(usage) = data.get("usage") {
            state["usage"] = usage.clone();
        }
        let end = state["end_offset"].as_u64().unwrap();
        let next = data["end_offset"].as_u64().unwrap();
        state["end_offset"] = json!(end.max(next));
    }
}
fn protected_live(frame: &Value) -> bool {
    frame["event"] == "live"
        && frame["data"] != empty_client(frame["data"]["key"].as_str().unwrap())["live"]
}
struct RowCheck {
    actual: Value,
    original: Value,
    actual_live: Vec<Value>,
    original_live: Vec<Value>,
    case: String,
    origin: String,
    seed: String,
    differences: Vec<Value>,
}
impl RowCheck {
    fn new(cap: &Cap, origin: &str, seed: Option<&str>) -> Self {
        let key = "SESSION:chair";
        let mut state = json!({(key):empty_client(key)});
        let mut source = "empty".to_owned();
        if let Some(name) = seed {
            let path = format!("out-1/{}/{name}", cap.case);
            let page = frozen::parse(name, &String::from_utf8(frozen::bytes(&path)).unwrap());
            if page["status"] == 200 {
                let body = &page["body"];
                apply_frames(
                    &mut state,
                    &[json!({"event":"rows","data":{
                    "key":key,"upsert":body["rows"],"remove":[],"end_offset":body["page"]["end_offset"],
                    "display":body["display"],"patch":body["patch"],"find":body["find"],"images":body["images"],"usage":body["usage"]}})],
                );
                source = path;
            } else {
                source = format!("empty ({path} returned {})", page["status"]);
            }
        }
        Self {
            actual: state.clone(),
            original: state,
            actual_live: Vec::new(),
            original_live: Vec::new(),
            case: cap.case.clone(),
            origin: origin.into(),
            seed: source,
            differences: Vec::new(),
        }
    }
    fn observe(&mut self, name: &str, actual: &[Value], original: &[Value]) {
        apply_frames(&mut self.actual, actual);
        apply_frames(&mut self.original, original);
        self.actual_live
            .extend(actual.iter().filter(|frame| protected_live(frame)).cloned());
        self.original_live.extend(
            original
                .iter()
                .filter(|frame| protected_live(frame))
                .cloned(),
        );
        let different: Vec<_> = (0..actual.len().max(original.len()))
            .filter(|&i| actual.get(i) != original.get(i))
            .collect();
        if !different.is_empty() {
            let counts = |frames: &[Value]| json!({"frames":frames.len(),"upserts":frames.iter().map(|f|f["data"]["upsert"].as_array().map_or(0,Vec::len)).sum::<usize>(),"empty_live":frames.iter().filter(|f|f["event"]=="live" && !protected_live(f)).count()});
            self.differences.push(json!({"capture":name,"different_positions":different,"actual":counts(actual),"ORIGINAL":counts(original)}));
        }
    }
}
impl Drop for RowCheck {
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }
        assert_eq!(
            self.actual, self.original,
            "row client state {}/{}",
            self.case, self.origin
        );
        assert_eq!(
            self.actual_live, self.original_live,
            "ordered live frames {}/{}",
            self.case, self.origin
        );
        let count: usize = self
            .differences
            .iter()
            .map(|d| d["different_positions"].as_array().unwrap().len())
            .sum();
        let record = json!({"seed":self.seed,"accepted_frame_positions":count,"differences":self.differences});
        if let Some(root) = std::env::var_os("S9D_RECORD") {
            let path = Path::new(&root)
                .join("row-differences")
                .join(&self.case)
                .join(format!("{}.txt", self.origin));
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(
                path,
                format!("{}\n", serde_json::to_string_pretty(&record).unwrap()),
            )
            .unwrap();
        }
    }
}
fn assert_capture(mode: &str, actual: &Value) {
    let mut actual = without_body(actual.clone());
    let mut expected = frozen::expected(&[&format!("{mode}/")]);
    for values in [&mut actual, &mut expected] {
        for (name, value) in values.as_object_mut().unwrap() {
            if name.ends_with(".json") {
                value["rows"] = Value::Null;
            }
        }
    }
    assert_eq!(actual, expected);
}
#[test]
fn row_reducer_keeps_state_after_a_repeated_upsert() {
    let key = "SESSION:chair";
    let frame = json!({"event":"rows","data":{"key":key,"upsert":[{"id":"a","text":"hello","order":[0,0]}],"remove":[],"end_offset":1,"usage":null,"display":{},"patch":{},"find":{},"images":{}}});
    let mut state = json!({(key):empty_client(key)});
    apply_frames(&mut state, std::slice::from_ref(&frame));
    let expected = state.clone();
    apply_frames(&mut state, &[frame]);
    assert_eq!(state, expected);
}
#[test]
#[should_panic(expected = "changed row text")]
fn row_reducer_rejects_changed_row_text() {
    let key = "SESSION:chair";
    let frame = json!({"event":"rows","data":{"key":key,"upsert":[{"id":"a","text":"hello","order":[0,0]}],"remove":[],"end_offset":1,"usage":null,"display":{},"patch":{},"find":{},"images":{}}});
    let mut wrong = frame.clone();
    wrong["data"]["upsert"][0]["text"] = json!("wrong");
    let mut original = json!({(key):empty_client(key)});
    let mut actual = original.clone();
    apply_frames(&mut original, &[frame]);
    apply_frames(&mut actual, &[wrong]);
    assert_eq!(actual, original, "changed row text");
}
