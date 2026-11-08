pub fn queue_labels(index: usize, mode: &str) -> (String, String) {
    let mut characters = mode.chars();
    let detail = characters
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + characters.as_str())
        .unwrap_or_default();
    (format!("queue-{index}"), detail)
}
