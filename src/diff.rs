//! A unified line diff for the hook plan (ADR 0035), so the owner sees each line that setup
//! changes before they consent.

const CONTEXT: usize = 3;

/// The unified diff of `old` and `new` with three lines of context, or "" when they are equal.
/// ponytail: the longest-common-subsequence table is O(lines²) over the lines between the equal
/// start and end; a hook plan only adds, so that middle is small. A change only to the final
/// newline shows no hunk.
pub fn unified(path: &str, old: &str, new: &str) -> String {
    if old == new {
        return String::new();
    }
    let (all_old, all_new): (Vec<&str>, Vec<&str>) = (old.lines().collect(), new.lines().collect());
    let start = all_old
        .iter()
        .zip(&all_new)
        .take_while(|(a, b)| a == b)
        .count();
    let end = all_old[start..]
        .iter()
        .rev()
        .zip(all_new[start..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let (old, new) = (
        &all_old[start..all_old.len() - end],
        &all_new[start..all_new.len() - end],
    );
    // common[i][j] is the length of the longest common subsequence of old[i..] and new[j..].
    let mut common = vec![vec![0usize; new.len() + 1]; old.len() + 1];
    for i in (0..old.len()).rev() {
        for j in (0..new.len()).rev() {
            common[i][j] = match old[i] == new[j] {
                true => common[i + 1][j + 1] + 1,
                false => common[i + 1][j].max(common[i][j + 1]),
            };
        }
    }
    let mut lines: Vec<(char, &str)> = all_old[..start].iter().map(|line| (' ', *line)).collect();
    let (mut i, mut j) = (0, 0);
    while i < old.len() || j < new.len() {
        if i < old.len() && j < new.len() && old[i] == new[j] {
            lines.push((' ', old[i]));
            (i, j) = (i + 1, j + 1);
        } else if i < old.len() && (j == new.len() || common[i + 1][j] >= common[i][j + 1]) {
            // A removal comes before the addition that replaces it, as `diff -u` orders them.
            lines.push(('-', old[i]));
            i += 1;
        } else {
            lines.push(('+', new[j]));
            j += 1;
        }
    }
    lines.extend(
        all_old[all_old.len() - end..]
            .iter()
            .map(|line| (' ', *line)),
    );

    let mut out = format!("--- {path}\n+++ {path}\n");
    let changed: Vec<usize> = (0..lines.len()).filter(|&k| lines[k].0 != ' ').collect();
    let mut k = 0;
    while k < changed.len() {
        let start = changed[k].saturating_sub(CONTEXT);
        let mut end = (changed[k] + CONTEXT + 1).min(lines.len());
        // Changes whose context meets go in one hunk.
        while k + 1 < changed.len() && changed[k + 1] <= end + CONTEXT {
            k += 1;
            end = (changed[k] + CONTEXT + 1).min(lines.len());
        }
        k += 1;
        let count =
            |range: &[(char, &str)], skip: char| range.iter().filter(|l| l.0 != skip).count();
        let (old_before, new_before) = (count(&lines[..start], '+'), count(&lines[..start], '-'));
        let (old_count, new_count) = (
            count(&lines[start..end], '+'),
            count(&lines[start..end], '-'),
        );
        // As `diff -u` writes a range: an empty side starts at the line before it, and a count of
        // one is left out.
        let range = |before: usize, count: usize| match count {
            1 => format!("{}", before + 1),
            _ => format!("{},{count}", before + usize::from(count > 0)),
        };
        out += &format!(
            "@@ -{} +{} @@\n",
            range(old_before, old_count),
            range(new_before, new_count)
        );
        for (mark, text) in &lines[start..end] {
            out += &format!("{mark}{text}\n");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::unified;

    #[test]
    fn equal_texts_have_no_diff() {
        assert_eq!(unified("config.toml", "a\n", "a\n"), "");
    }

    #[test]
    fn a_new_file_is_one_hunk_of_additions() {
        assert_eq!(
            unified("hooks.json", "", "{\n}\n"),
            "--- hooks.json\n+++ hooks.json\n@@ -0,0 +1,2 @@\n+{\n+}\n"
        );
    }

    #[test]
    fn far_apart_changes_are_two_hunks_with_three_lines_of_context() {
        let old: String = (1..=20).map(|n| format!("{n}\n")).collect();
        let new = old
            .replacen("\n2\n", "\ntwo\n", 1)
            .replacen("18\n", "18\neighteen\n", 1);
        assert_eq!(
            unified("f", &old, &new),
            "--- f\n+++ f\n\
             @@ -1,5 +1,5 @@\n 1\n-2\n+two\n 3\n 4\n 5\n\
             @@ -16,5 +16,6 @@\n 16\n 17\n 18\n+eighteen\n 19\n 20\n"
        );
    }

    /// The same diff as `diff -u` for a mixed change, so the app's diff view can parse it.
    #[test]
    fn matches_the_system_diff() {
        for (old, new) in [
            (
                "model = \"o3\"\n[a]\nx = 1\n[b]\ny = 2\nz = 3\n",
                "model = \"o3\"\n[a]\nx = 2\n[b]\ny = 2\n[c]\nw = 1\n",
            ),
            ("model = \"o3\"\n", "model = \"o3\"\n\n[a]\nx = 1\n"),
            ("a\nb\nc\n", "b\n"),
        ] {
            assert_eq!(
                unified("f", old, new),
                system_diff(old, new),
                "{old:?} {new:?}"
            );
        }
    }

    fn system_diff(old: &str, new: &str) -> String {
        let dir = std::env::temp_dir().join(format!("swarm-diff-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("old"), old).unwrap();
        std::fs::write(dir.join("new"), new).unwrap();
        let system = std::process::Command::new("diff")
            .args(["-u", "--label", "f", "--label", "f", "old", "new"])
            .current_dir(&dir)
            .output()
            .unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        String::from_utf8(system.stdout).unwrap()
    }
}
