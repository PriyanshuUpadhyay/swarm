mod tab_fixture;
use std::collections::BTreeSet;
use swarm::view::{
    choices::OwnerChoices,
    tabs::{Grouping, TabGroup, TabStrip},
};
use tab_fixture::*;

#[test]
fn opening() {
    let original = strip(&["design", "review"]);
    assert_eq!(
        original.opening("fix", Some("design")).open,
        ["design", "fix", "review"]
    );
    assert_eq!(original.opening("design", Some("review")), original);
    assert_eq!(
        original.opening("fix", Some("missing")).open,
        ["design", "review", "fix"]
    );
    assert_eq!(TabStrip::default().opening("design", None).open, ["design"]);
}
#[test]
fn closing_and_pruning() {
    let original = TabStrip {
        open: vec![
            "design".into(),
            "review".into(),
            "fix".into(),
            "design".into(),
        ],
        groups: vec![
            group("planning", &["review", "design"]),
            group("repair", &["fix"]),
        ],
    };
    let closed = original.closing("fix");
    assert_eq!(closed.open, ["design", "review"]);
    assert_eq!(closed.groups.len(), 1);
    assert_eq!(closed.groups[0].members, ["design", "review"]);
    assert_eq!(original.pruned(&["fix".into()].into()).open, ["fix"]);
    assert!(original.pruned(&BTreeSet::new()).groups.is_empty());
    assert_eq!(closed.closing("absent"), closed);
}
#[test]
fn moving() {
    let original = strip(&["design", "review", "fix"]);
    assert_eq!(
        original.moving("design", 2).open,
        ["review", "fix", "design"]
    );
    assert_eq!(original.moving("fix", 0).open, ["fix", "design", "review"]);
    for (key, index) in [
        ("review", 1),
        ("missing", 0),
        ("design", usize::MAX),
        ("design", 3),
    ] {
        assert_eq!(original.moving(key, index), original);
    }
}
#[test]
fn menu_moves_and_targets() {
    let mut original = strip(&["pending", "design", "review", "fix", "notes"]);
    original.groups.push(group("review", &["review", "fix"]));
    original.groups[0].folded = true;
    let targets = original.step_targets(&["pending".into()].into());
    assert!(!targets.contains_key("pending"));
    assert_eq!(targets["design"], (None, Some("fix".into())));
    assert_eq!(targets["review"], (None, Some("fix".into())));
    assert_eq!(targets["fix"], (Some("review".into()), None));
    assert_eq!(targets["notes"], (Some("review".into()), None));
    let original = original.closing("pending");
    assert_eq!(
        original.stepping("design", "fix").open,
        ["review", "fix", "design", "notes"]
    );
    assert_eq!(
        original.stepping("notes", "review").open,
        ["design", "notes", "review", "fix"]
    );
    assert_eq!(original.stepping("design", "fix").groups, original.groups);
    assert_eq!(original.stepping("review", "design"), original);
    assert_eq!(original.stepping("fix", "notes"), original);
    assert_eq!(
        original.stepping("review", "fix").groups[0].members,
        ["fix", "review"]
    );
    assert_eq!(original.stepping("missing", "design"), original);
    assert_eq!(original.stepping("design", "missing"), original);
    assert!(
        TabStrip::default()
            .step_targets(&BTreeSet::new())
            .is_empty()
    );
    assert_eq!(
        strip(&["design"]).step_targets(&BTreeSet::new())["design"],
        (None, None)
    );
}
#[test]
fn duplicate_membership() {
    let original = TabStrip {
        open: strip(&["design", "review", "fix", "notes"]).open,
        groups: vec![
            group("planning", &["design", "review"]),
            group("repair", &["review", "fix"]),
        ],
    };
    let targets = original.step_targets(&BTreeSet::new());
    assert_eq!(targets["design"].1.as_deref(), Some("review"));
    assert_eq!(targets["review"], (Some("design".into()), None));
    assert_eq!(targets["fix"], (None, None));
    let moved = original.stepping("design", "review");
    assert_eq!(moved.open, ["review", "design", "fix", "notes"]);
    assert_eq!(moved.groups[0].members, ["review", "design"]);
    assert_eq!(moved.groups[1].members, ["fix"]);
}
#[test]
fn tab_persistence_and_absent_fields() {
    let mut owner = OwnerChoices::default();
    owner.tabs.insert(
        "/repo".into(),
        TabStrip {
            open: strip(&["design", "review"]).open,
            groups: vec![group("planning", &["design"])],
        },
    );
    assert_eq!(
        serde_json::from_value::<OwnerChoices>(serde_json::to_value(&owner).unwrap()).unwrap(),
        owner
    );
    assert!(
        serde_json::from_str::<OwnerChoices>("{}")
            .unwrap()
            .tabs
            .is_empty()
    );
    assert_eq!(
        serde_json::from_str::<TabStrip>("{}").unwrap(),
        TabStrip::default()
    );
    assert_eq!(
        serde_json::from_str::<TabStrip>(r#"{"open":["design"]}"#).unwrap(),
        strip(&["design"])
    );
    assert!(
        serde_json::from_str::<TabStrip>(r#"{"groups":[]}"#)
            .unwrap()
            .open
            .is_empty()
    );
    let g: TabGroup = serde_json::from_str(
        r#"{"id":"planning","name":"Planning","color":"future","members":["design"]}"#,
    )
    .unwrap();
    assert_eq!(g.color, "grey");
    assert!(!g.folded);
}
#[test]
fn grouping_actions_and_boundaries() {
    let original = strip(&["design", "review", "fix", "notes"]);
    let planning = original.grouping(Grouping::New {
        id: "planning".into(),
        name: " Planning ".into(),
        color: "blue".into(),
        tab: "design".into(),
    });
    assert_eq!(planning.groups[0].name, "Planning");
    let joined = planning.grouping(Grouping::Add {
        tab: "fix".into(),
        group: "planning".into(),
    });
    assert_eq!(joined.open, ["design", "fix", "review", "notes"]);
    assert_eq!(joined.groups[0].members, ["design", "fix"]);
    let renamed = joined
        .grouping(Grouping::Rename {
            group: "planning".into(),
            name: "Build".into(),
        })
        .grouping(Grouping::Color {
            group: "planning".into(),
            color: "green".into(),
        })
        .grouping(Grouping::Fold {
            group: "planning".into(),
            folded: true,
        });
    assert_eq!(renamed.groups[0].name, "Build");
    assert_eq!(renamed.groups[0].color, "green");
    assert!(renamed.groups[0].folded);
    let second = renamed.grouping(Grouping::New {
        id: "reading".into(),
        name: "Reading".into(),
        color: "purple".into(),
        tab: "review".into(),
    });
    let transferred = second.grouping(Grouping::Add {
        tab: "fix".into(),
        group: "reading".into(),
    });
    assert_eq!(transferred.open, ["design", "review", "fix", "notes"]);
    assert_eq!(transferred.groups[0].members, ["design"]);
    assert_eq!(transferred.groups[1].members, ["review", "fix"]);
    assert_eq!(
        transferred
            .grouping(Grouping::Remove("review".into()))
            .groups[1]
            .members,
        ["fix"]
    );
    assert_eq!(
        transferred
            .grouping(Grouping::Remove("design".into()))
            .groups[0]
            .id,
        "reading"
    );
    let deleted = transferred.grouping(Grouping::Delete("reading".into()));
    assert_eq!(deleted.open, transferred.open);
    assert_eq!(deleted.groups.len(), 1);
    for action in [
        Grouping::New {
            id: "planning".into(),
            name: "Duplicate".into(),
            color: "red".into(),
            tab: "review".into(),
        },
        Grouping::New {
            id: "empty".into(),
            name: "  ".into(),
            color: "red".into(),
            tab: "review".into(),
        },
        Grouping::New {
            id: "missing".into(),
            name: "Missing".into(),
            color: "red".into(),
            tab: "absent".into(),
        },
        Grouping::Add {
            tab: "review".into(),
            group: "absent".into(),
        },
        Grouping::Add {
            tab: "absent".into(),
            group: "planning".into(),
        },
        Grouping::Rename {
            group: "planning".into(),
            name: "  ".into(),
        },
        Grouping::Rename {
            group: "absent".into(),
            name: "Name".into(),
        },
        Grouping::Color {
            group: "absent".into(),
            color: "red".into(),
        },
        Grouping::Fold {
            group: "absent".into(),
            folded: true,
        },
        Grouping::Remove("absent".into()),
        Grouping::Delete("absent".into()),
    ] {
        assert_eq!(planning.grouping(action), planning);
    }
}
#[test]
fn contiguous_group_order() {
    let grouped = TabStrip {
        open: strip(&["design", "review", "fix", "notes"]).open,
        groups: vec![group("planning", &["design", "review", "fix"])],
    };
    assert_eq!(
        grouped.opening("draft", Some("design")).open,
        ["design", "review", "fix", "draft", "notes"]
    );
    assert_eq!(
        grouped.moving("fix", 0).groups[0].members,
        ["fix", "design", "review"]
    );
    let outside = grouped.moving("review", 3);
    assert_eq!(outside.open, ["design", "fix", "notes", "review"]);
    assert_eq!(outside.groups[0].members, ["design", "fix"]);
    let inside = grouped.moving("notes", 1);
    assert_eq!(inside.groups[0].members, inside.open);
    assert_eq!(
        grouped.closing("review").groups[0].members,
        ["design", "fix"]
    );
    assert!(
        grouped
            .closing("design")
            .closing("review")
            .closing("fix")
            .groups
            .is_empty()
    );
    let scattered = TabStrip {
        open: strip(&["design", "notes", "review", "fix"]).open,
        groups: grouped.groups.clone(),
    };
    assert_eq!(
        scattered
            .pruned(&scattered.open.iter().cloned().collect())
            .open,
        grouped.open
    );
}
