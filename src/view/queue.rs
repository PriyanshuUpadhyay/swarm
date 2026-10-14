use serde_json::Value;

pub fn is_owner(text: &str) -> bool {
    !text.starts_with("<task-notification") && !text.starts_with("<agent-message")
}

fn apply(event: &Value, queue: &mut Vec<(String, bool)>) {
    if event["type"] != "queue_operation" {
        return;
    }
    match event["operation"].as_str() {
        Some("enqueue") => {
            if let Some(text) = event["content"].as_str() {
                queue.push((text.into(), false));
            }
        }
        Some("dequeue") if !queue.is_empty() => {
            queue.remove(0);
        }
        Some("remove" | "popAll") => {
            if let Some(index) = queue
                .iter()
                .position(|(text, _)| Some(text.as_str()) == event["content"].as_str())
            {
                queue.remove(index);
            }
        }
        _ => (),
    }
}

pub fn replay<'a>(events: impl IntoIterator<Item = &'a Value>) -> Vec<String> {
    let mut queue = Vec::new();
    for event in events {
        apply(event, &mut queue);
    }
    queue.into_iter().map(|(text, _)| text).collect()
}

pub fn can_pull_back(provider: Option<&str>, adapter: &str) -> bool {
    provider == Some("claude") && matches!(adapter, "tmux" | "tmux-swarm")
}

#[derive(Debug, PartialEq, Eq)]
pub enum PullBack {
    Pulled {
        owner: Vec<String>,
        popped: Vec<String>,
    },
    AlreadySent {
        popped: Vec<String>,
    },
    Waiting,
    Unconfirmed,
}
impl PullBack {
    pub fn decide<'a>(
        queue: &[String],
        after: impl IntoIterator<Item = &'a Value>,
        past_deadline: bool,
    ) -> Self {
        let mut open = queue
            .iter()
            .map(|text| (text.clone(), is_owner(text)))
            .collect();
        let mut popped = Vec::new();
        for event in after {
            apply(event, &mut open);
            if event["type"] == "queue_operation"
                && event["operation"] == "popAll"
                && let Some(text) = event["content"].as_str()
            {
                popped.push(text.to_owned());
            }
        }
        if open.iter().any(|(_, waits)| *waits) {
            return if past_deadline {
                Self::Unconfirmed
            } else {
                Self::Waiting
            };
        }
        let owner: Vec<_> = popped
            .iter()
            .filter(|text| is_owner(text))
            .cloned()
            .collect();
        if owner.is_empty() {
            Self::AlreadySent { popped }
        } else {
            Self::Pulled { owner, popped }
        }
    }
    pub fn clear_presses(&self, last: &str) -> usize {
        match self {
            Self::Pulled { popped, .. } => clear_presses(popped),
            Self::AlreadySent { popped } => {
                clear_presses(popped).max(clear_presses(&[last.to_owned()]))
            }
            Self::Waiting => clear_presses(&[last.to_owned()]),
            Self::Unconfirmed => 0,
        }
    }
}
pub fn clear_presses(texts: &[String]) -> usize {
    2 * texts.join("\n").split('\n').count() + 1
}
