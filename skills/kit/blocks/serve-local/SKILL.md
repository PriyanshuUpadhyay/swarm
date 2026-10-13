---
name: serve-local
description: "Serve a built page at a local URL with the project's own preview command or a loopback-only server, and record its build and stop command. Use when a page cannot open from a file."
license: MIT
metadata:
  kind: step
  owns: "a local view of the checked build"
  serves: "view and talk"
  needs: "a built folder and its commit"
  gives: "a URL, the served folder, the build commit, and the stop command"
  conflicts: ""
  users: "page-comments, build-canvas, planner"
  source: "docs/blocks.md (serve-local row, view and talk stage); blocks/build-canvas/SKILL.md (Delivery: ES modules fail over file://); mined rows s4#34, s4#45"
---

## Rules
- Use this block only when the page needs a URL, such as for ES modules, a fetch of local files, or a dev server. One HTML file opens from its path.
- Use the project's own preview command first (`npm run preview`, `vite preview`, or the README's command). With none, run `python3 -m http.server <port> --bind 127.0.0.1 --directory <folder>`. Never bind to `0.0.0.0` or a public address.
- Serve the build that the checks ran on. Record the URL, the folder, the commit (`git rev-parse --short HEAD`, and say so when the tree has changes), and the stop command with the process id.
- The server runs until you stop it, so start it where a tool time limit cannot stop it. Check that `curl -sI <url>` answers before you give the URL to anyone.
- When the step that needs the server ends, stop it, and check that the port is free.

## Example
Initial state: a story reel is a folder with `main.js` as an ES module, at commit `4f2a9c1`.
Action: the project has no preview command, so run `python3 -m http.server 8123 --bind 127.0.0.1 --directory site` and check it with `curl -sI http://127.0.0.1:8123/`.
Result: the step records `http://127.0.0.1:8123/`, `site`, `4f2a9c1`, and `kill 51234`. After the owner's review, the server stops and port 8123 is free.

## Check (script)
- none: the server lives only while its step runs.

## Check (judge)
- Does the step record the URL, the folder, the commit, and the stop command, and show that the server was stopped at the end? Quote each one.
