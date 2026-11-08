//! Public-API regression for the nesting bound (Step 5(g) slice S3, contract V4 items 1 and 3).
//!
//! `display` and `display_with_breaks` map a nested tree with recursion, so a deep input aborts the process with a
//! stack overflow. The level of a node is the number of steps from the root, so a child of the root is level 1. The
//! rule is that an input with any node, block or inline, above level 100 gives an error, "markdown nests deeper than
//! 100 levels", and no tree, while an input whose deepest node is at level 100 still gives a tree. Expected values
//! come from the contract rows, never from candidate output. Only the public results are compared. Every public
//! outcome of the test is collected before its single final assertion.
//!
//! The inputs are built from repeated markers: `> ` is a quote marker, `- ` is a bullet marker, and `*a ` and ` a*` open
//! and close one emphasis.

use swarm::view::transcript::markdown::display::{Display, display, display_with_breaks};

/// What one call gives.
#[derive(Debug, PartialEq)]
enum Observed {
    /// The call returned a tree.
    Tree,
    /// The call failed with this reason.
    Error(String),
}

fn observe(result: Result<Display, markdown::message::Message>) -> Observed {
    match result {
        Ok(_) => Observed::Tree,
        Err(message) => Observed::Error(message.reason),
    }
}

fn both(source: &str) -> (Observed, Observed) {
    (
        observe(display(source)),
        observe(display_with_breaks(source)),
    )
}

fn too_deep() -> Observed {
    Observed::Error("markdown nests deeper than 100 levels".to_owned())
}

#[test]
fn deep_nesting_gives_no_tree() {
    // 98 quote markers, then `x`, 197 B: 98 Blockquote levels, the Paragraph at level 99, the Text at level 100.
    let quotes_to_level_100 = format!("{}x", "> ".repeat(98));
    // 99 quote markers, then `x`, 199 B: the Text is at level 101.
    let quotes_to_level_101 = format!("{}x", "> ".repeat(99));
    // 101 bullet markers, then `x`, 203 B: 101 nested list items, each a List and a ListItem, far past level 100.
    let bullets_over_the_bound = format!("{}x", "- ".repeat(101));
    // 101 nested emphasis in one paragraph, 607 B: the Paragraph at level 1, the Emphasis at levels 2 to 102, the Text
    // at level 103. Only inline nodes are deep here.
    let emphasis_over_the_bound = format!("{}x{}", "*a ".repeat(101), " a*".repeat(101));

    let observed = vec![
        ("quotes_to_level_100", both(&quotes_to_level_100)),
        ("quotes_to_level_101", both(&quotes_to_level_101)),
        ("bullets_over_the_bound", both(&bullets_over_the_bound)),
        ("emphasis_over_the_bound", both(&emphasis_over_the_bound)),
    ];

    assert_eq!(
        observed,
        vec![
            ("quotes_to_level_100", (Observed::Tree, Observed::Tree)),
            ("quotes_to_level_101", (too_deep(), too_deep())),
            ("bullets_over_the_bound", (too_deep(), too_deep())),
            ("emphasis_over_the_bound", (too_deep(), too_deep())),
        ],
    );
}
