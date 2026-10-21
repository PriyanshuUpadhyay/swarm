---
name: serve-local
description: "Serve a project's build with its own dev or preview command on a loopback URL, and record its stop command. Use when a page needs a dev server, module scripts, storage, or its own files."
license: MIT
metadata:
  kind: step
  owns: "a local view of the checked build"
  serves: "view and talk"
  needs: "a built folder and its commit"
  gives: "a URL, the served folder, the build commit, and the stop command"
  conflicts: ""
  users: "page-comments, page-shelf, build-canvas, planner"
  source: "docs/blocks-catalog.md (serve-local row); docs/blocks.md (view and talk stage); blocks/build-canvas/SKILL.md (Delivery: ES modules fail over file://); mined rows s4#34, s4#45"
---

## Rules
- Use this block only for a page that the `page-shelf` block cannot carry: one that needs a dev server, module scripts, local storage, or a fetch of its own files. Give every other page by `page-shelf`.
- Use the project's own dev or preview command (`npm run preview`, `vite preview`, or the README's command), bound to `127.0.0.1`. A folder with no such command is not served here. Make it one self-contained page for `page-shelf`, or give it its own dev command first. Never bind to `0.0.0.0` or a public address.
- Serve the build that the checks ran on. Record the URL, the folder, the commit (`git rev-parse --short HEAD`, and say so when the tree has changes), and the stop command with the process id.
- The server runs until you stop it, so start it where a tool time limit cannot stop it. Check that `curl -sI <url>` answers before you give the URL to anyone.
- When the step that needs the server ends, stop it, and check that the port is free.

## Example
Initial state: a story reel is a Vite project with `main.js` as an ES module, at commit `4f2a9c1`.
Action: run `npm run preview -- --host 127.0.0.1 --port 8123` and check it with `curl -sI http://127.0.0.1:8123/`.
Result: the step records `http://127.0.0.1:8123/`, the project folder, `4f2a9c1`, and `kill 51234`. After the owner's review, the server stops and port 8123 is free.

## Check (script)
- none: the server lives only while its step runs.

## Check (judge)
- Does the step record the URL, the folder, the commit, and the stop command, and show that the server was stopped at the end? Quote each one.
