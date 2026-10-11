# agent-kit

Workflow skills for coding agents (Claude Code, Codex, Antigravity). Each skill is a folder with a
`SKILL.md` that the agent loads when the task matches its description.

## Skills

| Skill | Use |
|---|---|
| `pair` | Write code with the user, one approved diff at a time |
| `deliver` | Drive one named task to a checkable exit condition, stop before a push or merge |
| `flow` | Run one feature through frame, design, contracts, impact, build and close, one status file per step (on trial) |
| `research` | Answer one question from evidence in a short report |
| `web-search` | Three visible seats search different parts of the web, the chair merges |
| `council` | A visible debate between Claude, GPT and Gemini voices that ends in a verdict |
| `review-check` | Judge a range or local changes; a script gives the verdict only when every changed unit has a row |
| `review-walk` | Walk a PR with review-check findings drafted in your voice into a pending review |
| `decisions` | Record consequential choices as in-repo ADRs |
| `cleanup-gate` | A full cleanup pass, only on request |
| `engineering-standards` | Scoped engineering rules for design, code and review |
| `prove-it-works` | What counts as proof that a change works |
| `sequence-verifiable-units` | Order multi-step work so each step can be checked |
| `minimize-reader-load` | When code gets its own function, file or module |
| `ink` | Pictures, generative plates, films, and websites drawn by code, from a story or a visual reference |
| `orchestrate-claude`, `orchestrate-codex`, `orchestrate-agy` | Runtime adapters that bind the skills' worker needs to each agent CLI |

`references/plan-layout.md` is the plan shape and the first screen of a long result that several
skills point to.
`contracts/orchestration-requirements.schema.json` describes what a skill may ask of a runtime adapter.

## Install

Link the skills you want into your agent's skills folder, and the references next to them:

```sh
git clone --recurse-submodules https://github.com/PriyanshuUpadhyay/agent-kit ~/agent-kit
mkdir -p ~/.claude/skills ~/.claude/references
for s in ~/agent-kit/skills/*/ ~/agent-kit/vendor/taste-skill/skills/taste-skill/ \
  ~/agent-kit/vendor/emil-skills/skills/animate/ ~/agent-kit/vendor/emil-skills/skills/break-ui/; do
  ln -sfn "$s" ~/.claude/skills/"$(basename "$s")"
done
ln -sfn ~/agent-kit/references/plan-layout.md ~/.claude/references/plan-layout.md
```

Codex reads `~/.agents/skills` and Antigravity reads `~/.gemini/config/skills`.

## What the skills expect

- Skills that start workers (`council`, `web-search`, `research`) run them as visible panes through
  [swarm](https://github.com/PriyanshuUpadhyay/swarm).
- `flow`, `pair`, `deliver`, `review-check`, and `review-walk` keep their runs in
  `<repo-root>/tmp/<skill>/`, which `references/run-folder.md` describes. Other skills write under
  `~/.claude/` (`reports/`, `council-log/`) and create those folders on first use.
- `council` calls `~/.claude/scripts/ensure-council-access.py`, which is not part of this kit yet.

## Borrowed skills

These are the expert packs used next to this kit. Two of them are in `vendor/` as git submodules,
because `ink` reads their `taste-skill` and `animate` skills on every site build: Leonxlnx/taste-skill
and emilkowalski/skills. The others are not in this repo. To copy the setup,
take each pack at the commit shown, keep its license, and read its skills by path from the step
that needs them, instead of linking them all into every session.

| Pack | Commit | Take | Domain |
|---|---|---|---|
| [cloudflare/skills](https://github.com/cloudflare/skills) | `626547c` | `skills/` | Workers, Durable Objects |
| [AvdLee/SwiftUI-Agent-Skill](https://github.com/AvdLee/SwiftUI-Agent-Skill) | `b24e68a` | `skills/swiftui-expert-skill/` | SwiftUI and AppKit |
| [emilkowalski/skills](https://github.com/emilkowalski/skills) | `e8a175d` | `skills/` (`vendor/emil-skills`) | motion, Apple design, Swift |
| [phuryn/pm-skills](https://github.com/phuryn/pm-skills) | `8607e3b` | the `pm-*/` folders | product framing |
| [codeswithroh/tastemaker](https://github.com/codeswithroh/tastemaker) | `6bada3c` | `skills/tastemaker/` | web visuals |
| [nextlevelbuilder/ui-ux-pro-max-skill](https://github.com/nextlevelbuilder/ui-ux-pro-max-skill) | `09170ee` | `cli/assets/skills/` | design systems; `design` sends prompts to Gemini and MuAPI |
| [Leonxlnx/taste-skill](https://github.com/Leonxlnx/taste-skill) | `ce26fc2` | `skills/` without `output-skill` (`vendor/taste-skill`) | web visuals |
| [NSHipster/sosumi.ai](https://github.com/NSHipster/sosumi.ai) | `79337f5` | `public/SKILL.md` | Apple docs and HIG as Markdown |
| [vercel-labs/web-interface-guidelines](https://github.com/vercel-labs/web-interface-guidelines) | `e3d624b` | `command.md`, not `install.sh` | web UX rules |
| [pbakaus/impeccable](https://github.com/pbakaus/impeccable) | `9d715cc` | `plugin/skills/impeccable/`, not `hooks/` | web design checks; `scripts/impeccable detect <file>` runs 61 rules |
| [microsoft/rust-guidelines](https://github.com/microsoft/rust-guidelines) | `19723b3` | `src/guidelines/` | Rust rules; the `libs/` rules are for library APIs |
| [leonardomso/rust-skills](https://github.com/leonardomso/rust-skills) | `fd2a861` | `SKILL.md` and `rules/` | Rust 1.96, edition 2024 |
| [zaxified/zig-skills](https://github.com/zaxified/zig-skills) | `be65603` | `skills/zig/` | Zig 0.16.0 |
| [cursor/plugins](https://github.com/cursor/plugins) | `d7cde2b` | `pstack/skills/typescript-best-practices/` | TypeScript rules |
| [Glitch-Cat-Club/glitch-skills](https://github.com/Glitch-Cat-Club/glitch-skills) | `8a9a2df` | `glitch-walk/` and `LICENSE`, with a local patch | a walk page: one action as real screens and real code lines |
| [morluto/rea](https://github.com/morluto/rea) | `rea-agents-6.1.0` | `skill-src/reverse-engineer-anything/` and `LICENSE`, CLI pinned, no MCP | reverse engineering shipped apps, Electron, web, .NET |

Tools, not skill files: `xcrun mcpbridge` (Xcode), the sosumi MCP (`https://sosumi.ai/mcp`), Mobbin
through Composio (`composio link mobbin_mcp`, paid Mobbin plan), `npx @google/design.md lint`, the
`DESIGN.md` collection [awesome-claude-design](https://github.com/VoltAgent/awesome-claude-design), and
[swift-snapshot-testing](https://github.com/pointfreeco/swift-snapshot-testing).
Impeccable's hooks are left out, because they run on every edit in every session. Its launcher
downloads a binary on first use.

## Public-safety gate

`scripts/public-safety.sh` runs [gitleaks](https://github.com/gitleaks/gitleaks) with
`.gitleaks.toml` plus a private word list that never enters the repo. Enable the pre-commit hook
with `git config core.hooksPath .githooks`.

## License

MIT
