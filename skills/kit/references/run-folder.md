---
description: Where a skill keeps the files of a run that must outlive one chat (step files, task files). Read when a skill points here.
---

# Run folder

A skill that keeps a run's files across chats keeps them in `<repo-root>/tmp/<skill>/`.
`<repo-root>` is `git rev-parse --show-toplevel`, so each worktree has its own runs.

Example. A `pair` task in `~/work/app/wt/feat-login` writes
`~/work/app/wt/feat-login/tmp/pair/2026-09-30-login-rate-limit.md`. `git status` does not show it,
because `/tmp/` is in the clone's `.git/info/exclude`.

- Before the first write, make sure that `/tmp/` is a line in `.git/info/exclude`. That file is local
  to the clone, so the team never sees it, and one line covers every worktree:

  ```sh
  x="$(git rev-parse --git-common-dir)/info/exclude"
  rg -qxF '/tmp/' "$x" 2>/dev/null || echo '/tmp/' >> "$x"
  ```

- Private files that belong to no single checkout, such as repo profiles, repo rules, or a voice
  guide, stay in `~/.<skill>/`, which the user keeps in a private dotfiles repo.
- Removing a worktree removes its runs. Keep a result that must last longer in the PR, a commit,
  or an ADR.
- Outside a git repository, ask the user where the run goes.
- Short-lived scratch for one session goes in the session's scratch folder, not here.
