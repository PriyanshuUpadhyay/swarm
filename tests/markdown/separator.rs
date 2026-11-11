//! Public-API regression for the coordinated identifier-separator and GFM fallback-key repair (Unit12a, SEPARATOR-V3 section 6).
//!
//! Expected values come from the owner decisions, the ORIGINAL renderer and the source identity contract, never from candidate output.
//! Only public returned values are compared. Every public outcome of a test is collected before its single final assertion.

use markdown::mdast::Node;
use markdown::{Options, ParseOptions};

type Identifiers = (Vec<String>, Vec<String>);

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_string()).collect()
}

fn collect(
    node: &Node,
    definitions: &mut Vec<String>,
    references: &mut Vec<String>,
    text: &mut String,
) {
    match node {
        Node::Definition(definition) => definitions.push(definition.identifier.clone()),
        Node::LinkReference(reference) => references.push(reference.identifier.clone()),
        Node::Text(literal) => text.push_str(&literal.value),
        _ => {}
    }
    for child in node.children().into_iter().flatten() {
        collect(child, definitions, references, text);
    }
}

fn identifiers(tree: &Node) -> Identifiers {
    let (mut definitions, mut references) = (Vec::new(), Vec::new());
    collect(tree, &mut definitions, &mut references, &mut String::new());
    (definitions, references)
}

fn plain_text(tree: &Node) -> String {
    let mut text = String::new();
    collect(tree, &mut Vec::new(), &mut Vec::new(), &mut text);
    text
}

fn hrefs(html: &str) -> Vec<String> {
    html.split("href=\"")
        .skip(1)
        .map(|rest| rest.split('"').next().unwrap_or_default().to_string())
        .collect()
}

#[test]
fn an_ordinary_label_keeps_its_internal_separator() {
    let source = "[a b]\n\n[ab]: https://e.test/first\n[a b]: https://e.test/second";
    let with_hooks = swarm::view::transcript::markdown::parse(source)
        .map(|tree| identifiers(&tree))
        .map_err(|error| error.to_string());
    let default_mode = markdown::to_mdast(source, &ParseOptions::default())
        .map(|tree| identifiers(&tree))
        .map_err(|error| error.to_string());
    let html = hrefs(&markdown::to_html(source));

    let expected_identifiers: Identifiers = (strings(&["ab", "a b"]), strings(&["a b"]));
    assert_eq!(
        (with_hooks, default_mode, html),
        (
            Ok(expected_identifiers.clone()),
            Ok(expected_identifiers),
            strings(&["https://e.test/second"])
        ),
    );
}

#[test]
fn a_gfm_caret_label_is_one_whole_key_and_never_a_false_alias() {
    let matching = "[^ a]\n\n[^ a]: https://e.test/u";
    let different = "[^ a]\n\n[^a ]: https://e.test/u";
    let (parse, options) = (ParseOptions::gfm(), Options::gfm());

    let matching_tree = markdown::to_mdast(matching, &parse)
        .map(|tree| identifiers(&tree))
        .map_err(|error| error.to_string());
    let matching_html = markdown::to_html_with_options(matching, &options)
        .map(|html| hrefs(&html))
        .map_err(|error| error.to_string());
    let different_tree = markdown::to_mdast(different, &parse)
        .map(|tree| (identifiers(&tree), plain_text(&tree)))
        .map_err(|error| error.to_string());
    let different_html =
        markdown::to_html_with_options(different, &options).map_err(|error| error.to_string());

    assert_eq!(
        (matching_tree, matching_html, different_tree, different_html),
        (
            Ok((strings(&["^ a"]), strings(&["^ a"]))),
            Ok(strings(&["https://e.test/u"])),
            Ok(((strings(&["^a"]), Vec::new()), "[^ a]".to_string())),
            Ok("<p>[^ a]</p>\n".to_string()),
        ),
    );
}
