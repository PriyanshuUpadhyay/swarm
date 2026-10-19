#![allow(dead_code)]
use swarm::view::tabs::{TabGroup, TabStrip};

pub fn strip(open: &[&str]) -> TabStrip {
    TabStrip {
        open: open.iter().map(|s| (*s).into()).collect(),
        groups: Vec::new(),
    }
}
pub fn group(id: &str, members: &[&str]) -> TabGroup {
    TabGroup {
        id: id.into(),
        name: id.into(),
        color: "blue".into(),
        members: members.iter().map(|s| (*s).into()).collect(),
        folded: false,
    }
}
