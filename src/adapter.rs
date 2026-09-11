pub struct Adapter {
    pub spawn: String,
    pub ring: String,
    pub list: String,
    pub close: String,
}

pub fn parse(name: &str, text: &str) -> Result<Adapter, Box<dyn std::error::Error>> {
    let mut verbs = std::collections::HashMap::new();
    for line in text.lines().filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#')) {
        let (key, value) = line.split_once('=').ok_or(format!("adapter {name}: bad line {line:?}"))?;
        verbs.insert(key.trim(), value.trim().to_string());
    }
    let mut take = |verb: &str| verbs.remove(verb).ok_or(format!("adapter {name}: missing {verb}"));
    let adapter = Adapter { spawn: take("spawn")?, ring: take("ring")?, list: take("list")?, close: take("close")? };
    if let Some(key) = verbs.keys().next() {
        return Err(format!("adapter {name}: unknown key {key}").into());
    }
    Ok(adapter)
}
