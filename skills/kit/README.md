# agent-kit

Workflow skills for coding agents (Claude Code, Codex, Antigravity). Each skill is a folder with a
`SKILL.md` that the agent loads when the task matches its description.

## Skills

| Skill | Use |
|---|---|
| `plan` | The front door of every work verb: plan from the verb's preset, draw it, get approval, run the steps |
| `implement` | Run one change through frame, design, contracts, impact, build, review, and close, in `pair` or `auto` mode (on trial) |
| `research` | Answer one question from evidence in a short report |
| `web-search` | Three visible seats search different parts of the web, the chair merges |
| `council` | A visible debate between Claude, GPT and Gemini voices that ends in a verdict |
| `review` | Check a range with a script verdict, walk a PR into a pending review, or run a cleanup pass on request |
| `decisions` | Record consequential choices as in-repo ADRs |
| `engineering-standards` | Scoped engineering rules for design, code and review |
| `build` | Pictures, generative plates, films, and websites drawn by code, from a story or a visual reference |
| `explain` | Explain a subject with one running example, using the `voice-teach` block |
| `orchestrate-claude`, `orchestrate-codex`, `orchestrate-agy` | Runtime adapters that bind the skills' worker needs to each agent CLI |

## Removed skill names (2026-10-19)

Every work verb now starts from a preset through `plan` (see `skills/plan/SKILL.md`). Five skills
merged into two, and their run folders (`tmp/pair/`, `tmp/deliver/`, `tmp/review-check/`,
`tmp/review-walk/`) stay where they were.

| You typed | Type now |
|---|---|
| `/pair` | `/implement`, then choose `pair` mode |
| `/deliver` | `/implement`, then choose `auto` mode |
| `/review-check` | `/review`, then choose `check` |
| `/review-walk` | `/review`, then choose `walk` |
| `/cleanup-gate` | `/review`, then choose `cleanup` |

The rules for proof, checked units, and code extraction live directly in
`blocks/prove-it-works/SKILL.md`, `blocks/checked-units/SKILL.md`, and
`blocks/when-to-extract/SKILL.md`.

The runtime adapters, `swarm-orchestrator`, `swarm-voice`, `engineering-standards`, and `decisions`
set `user-invocable: false`. They stay available to the model. Claude Code hides them from its
slash menu, as [its skill controls](https://code.claude.com/docs/en/skills#control-who-invokes-a-skill)
specify. Swarm checks this field for its web menu when a provider lists the skill's file path.
pi lists `sourceInfo.path`, and Codex lists `path`. Claude's initialize command list has no file
path, so its own visibility filter and plugin label apply. The web composer uses each command
row's `source`; a file under the Swarm plugin's `skills/` folder gets `swarm` from its manifest.

`references/plan-layout.md` is the plan shape and the first screen of a long result that several
skills point to.
`contracts/orchestration-requirements.schema.json` describes what a skill may ask of a runtime adapter.

## Install

The kit is one plugin, `swarm`, for Claude Code, Codex, Antigravity and pi. Each CLI installs the
whole kit, so a skill reaches the kit's `blocks/` and `references/` two folders up from its own
folder. `swarm setup plugin` installs the kit that ships with swarm in each CLI on `PATH` after
you say yes. Run it again after an upgrade to refresh the copies.
`swarm doctor` reports stale copies, and `swarm launch` shows the fix in one line.
By hand, from a clone, the kit folder is its own marketplace:

```sh
claude plugin marketplace add <repo>/skills/kit && claude plugin install swarm@swarm
codex plugin marketplace add <repo>/skills/kit && codex plugin add swarm@swarm
agy plugin install <repo>/skills/kit
pi install <repo>/skills/kit
```

Claude Code shows each skill with the plugin name in front, for example `/swarm:implement`; pi shows
`/skill:implement`. Claude Code and pi read the local folder in place, so edits reach the next session.
Run the commands above after an edit to refresh the copies in Codex and Antigravity.
The package build stamps both plugin manifests from the kit content in `<repo>/scripts/stamp-kit.py`.
A source checkout keeps its ordinary manifest version and needs no stamp.

Do not link a kit skill folder into an agent's skills folder. A tool reads `../../blocks/` from the
link's own path, not from the kit, so the blocks are not found. The three vendor skills
(`taste-skill`, `animate`, `break-ui`) use no blocks, so link them:

```sh
mkdir -p ~/.claude/skills
for s in <repo>/skills/kit/vendor/taste-skill/skills/taste-skill/ \
  <repo>/skills/kit/vendor/emil-skills/skills/animate/ <repo>/skills/kit/vendor/emil-skills/skills/break-ui/; do
  ln -sfn "$s" ~/.claude/skills/"$(basename "$s")"
done
```

Codex reads `~/.agents/skills` and Antigravity reads `~/.gemini/config/skills`.

## What the skills expect

- Skills that start workers (`council`, `web-search`, `research`) run them as visible panes through
  [swarm](https://github.com/PriyanshuUpadhyay/swarm).
- `references/run-folder.md` owns run placement by kind, including repository state, reports,
  council input and logs, and piece artifacts. It also keeps the older paths used by `implement`
  and `build`. Each skill keeps its artifact names.
- `council` calls `~/.claude/scripts/ensure-council-access.py`, which is not part of this kit yet.

## Borrowed skills

These are the expert packs used next to this kit. Two of them are in `vendor/` as git submodules,
because `build` reads their `taste-skill` and `animate` skills on every site build: Leonxlnx/taste-skill
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
