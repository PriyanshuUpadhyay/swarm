---
status: accepted
date: 2026-11-19
deciders: [user]
related: ["0086", "0131", "0132"]
informed-by:
  - 'owner, 2026-11-19 (paraphrase): use option b, with explicit extensions for Pi panes that swarm launches'
---

# 0153. Swarm identifies Pi and loads its launch extensions explicitly

Pi had a host contract but no runtime line. The Claude Code hook inside the Claude bridge could
identify the turn as Claude, although Pi owns the turn and its tools. Swarm's Pi launch adds the
Pi runtime line to the appended system prompt for both chair and worker. The inner Claude start
hook does not add a contract when Pi started it. The Pi runtime entry in
[`run-workers`](../../skills/kit/blocks/run-workers/SKILL.md) owns the worker execution rule.

`swarm launch` starts Pi with `--no-extensions` and names the extension files in its launch code.
This keeps the owner's installed Claude bridge and command guard, plus Swarm's live output
extension. A picked Codex account also keeps the installed account shim. Herdr keeps its installed
Pi state reporter. Swarm uses the installed bridge entry point, not a temporary npm package source
that Pi may download or update. Other Pi providers do not need a bridge installation.

Pi 1.5.2 turns off automatic extension loading, including built-ins, with this flag. It still
loads skills from configured packages, including the kit. Pi can still resolve package resources,
so the flag is not a package-install or network block. This choice does not change the owner's Pi
settings, raw `swarm spawn` commands, or interactive Pi outside Swarm. The bridge entry point is a
path dependency: check it when updating the bridge package.

The owner later chose to add the tested subagent and background-process packages to the same
explicit launch set. `swarm setup pi` previews and installs the two exact package versions in
Swarm state. It does not edit Pi's settings. Launch checks the installed package files and
continues with a warning if setup has not finished. Launch loads the process package and sends
its shell starts through the Swarm guard and the owner's Bash rules. It does not load the
subagent package yet: its SDK child sessions can run Bash without inheriting the Swarm launch
guard. The built-in extension set did not change.

Swarm supports Pi 1.5.x. Swarm uses explicit extension and system-prompt flags, OSC 7501
pane state, and the extension API for live messages and pre-tool checks. These were checked
together on Pi 1.5.2. Doctor and launch share the Pi version probe. An unknown or unsupported
version is a warning, not a refusal; no version is currently listed as known to break.
