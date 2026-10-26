//! Board sources: each file source of a post gets its current git blob (ADR 0118).

use super::*;

/// A file source gets its current blob, and a named blob must match the file now, so a post
/// cannot cite a file version that the author did not read.
pub(crate) fn checked_sources(cwd: &str, sources: &[String]) -> Result<Vec<String>, String> {
    let top = || {
        git(cwd, &["rev-parse", "--show-toplevel"])
            .ok_or("swarm: a file source needs a git folder; cite a URL instead")
    };
    sources
        .iter()
        .map(|source| {
            if source.starts_with("https://") || source.starts_with("http://") {
                return Ok(source.clone());
            }
            let (path, named) = split_source(source);
            let top = top()?;
            let blob = git(&top, &["hash-object", "--", path])
                .filter(|b| !b.is_empty())
                .ok_or_else(|| format!("swarm: source {path} is not a readable file in {top}"))?;
            match named {
                Some(named) if !blob.starts_with(&named.to_ascii_lowercase()) => Err(format!(
                    "swarm: source {source} does not match the file now (blob {blob})"
                )),
                _ => Ok(format!("{path}@{blob}")),
            }
        })
        .collect()
}

/// A source as (path, named blob). Only a hex tail is a blob, so a path such as
/// `node_modules/@types/x.d.ts` stays whole.
fn split_source(source: &str) -> (&str, Option<&str>) {
    match source.rsplit_once('@') {
        Some((path, blob)) if !blob.is_empty() && blob.chars().all(|c| c.is_ascii_hexdigit()) => {
            (path, Some(blob))
        }
        _ => (source, None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_hex_tail_after_the_last_at_is_a_blob() {
        assert_eq!(
            split_source("src/a.rs@3f2a1c9"),
            ("src/a.rs", Some("3f2a1c9"))
        );
        assert_eq!(split_source("src/a.rs"), ("src/a.rs", None));
        let scoped = "node_modules/@types/node/index.d.ts";
        assert_eq!(split_source(scoped), (scoped, None));
        assert_eq!(split_source("a@b/c.rs@"), ("a@b/c.rs@", None));
    }
}
