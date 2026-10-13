---
name: build-site
description: "A website is built as one more form of a piece: sections replace shots, screen widths replace fps, and the loader, the scroll, and the page transitions carry the story."
license: MIT
metadata:
  kind: step
  owns: "what changes when the piece is a website: sections, widths, drivers, the grey-box page, and the loader"
  serves: "design, build"
  needs: "a brief, the look tokens, and the story sheet"
  gives: "a page in the site folder"
  conflicts: ""
  users: "ink"
  source: "skills/ink/references/site.md (scope, intake, look, story and motion, animatic, QA; moved 2026-10-13)"
---

## Rules
Scope
- This block builds landing pages, portfolios, product and launch pages, scroll stories, a 3D or shader hero, a loader, and page transitions.
- Login, stored form data, a database, a CMS, and dashboards are app work, which the repo's own workflow owns. This block can still deliver the hero or the loader as one component for that app.
Intake
- Sections replace shots. For each section, record its job (what the reader learns or does), its content, and its start, settled, and end state as the reader scrolls.
- Screen widths replace duration and fps. Design at the check widths of the `web-ui-basics` block, plus any width the user names.
- Record the page's drivers. Load and scroll follow the Drivers section of ink's `skills/ink/references/story-and-motion.md`, and a click that starts a page transition is a gate there. Hover, cursor, and any other click change a state, not a time, so list each state with the element it changes.
- A screen recording is the best site reference. Record one full scroll at each width. Extract frames at each section's settled state, at each transition midpoint, and across the loader.
- Research sources for sites: threeui.com (three.js site templates, some with their prompt), 21st.dev (components and page templates), and the GSAP showcase. A template's prompt is a `stated` source. Its rendered page is the `visible` one.
- When the page borrows a component or effect from a repo, record its licence and its last release date in the look file.
Look
- Write `DESIGN.md` beside the page code as the `look-grammar` block says, never under a run folder, because the code and other design tools read it there.
- For a page with 3D or texture, the look file also keeps light, 3D, texture, and the mark budget.
- A `look` token object in page code loads the tokens from `npx @google/design.md export --format dtcg DESIGN.md`, which keeps typography. The `css-vars` format drops it.
- A `DESIGN.md` pulled from a live site (the design-md-chrome extension) or taken from a collection, such as VoltAgent's on GitHub, is `stated` evidence. Measure the rendered pages before you trust its values.
- Read the `taste-skill` skill (Taste Skill, `design-taste-frontend`) for the layout and type rules that keep a page from looking like a template.
Story and motion
- The story sheet lists the sections in scroll order, each with the settled state that the scroll driver asks for.
- The motion list names the loader, the section entrances, the scroll-linked motion, hover and cursor states, and the page transitions.
- For UI timing and easing, read the `animate` skill from Emil Kowalski's pack.
Style frames and animatic
- Prove the look with the page frames of the `style-frames-concept` block.
- Build a grey-box page: every section as flat boxes with the real text sizes, the final scroll lengths, the loader, and the real transitions. Capture a scroll-through at both widths with `?shot=<section>&u=` frames, and check that each settled state reads.
QA
- The page meets the `web-ui-basics` block at each width.
- The loader ends on the painted hero, never on a blank frame. Record the time to the first painted hero on a throttled connection.

## Example
Initial state: the user screen-records a product site with a yellow page, a 3D handheld console as the hero, an ink-splash loader, and a page transition that tears the screen, and asks for "a site like this for my coffee brand".
Action: the sections become loader, hero, three feature sections, and footer, each with its settled state. The look becomes a `DESIGN.md` plus the 3D and texture rules in the look file. The style frames are the hero at 1440 px and 390 px beside the reference at the same widths.
Result: the coffee brand keeps its own name, logo, product, and copy. The page borrows the loader idea, the density, and the motion grammar.

## Check (script)
- none: the page checks are the script checks of the `web-ui-basics`, `look-grammar`, and `headless-shot` blocks.

## Check (judge)
- Does the step file list each section with its job and its start, settled, and end state? Quote the list.
- Does a shot taken as the loader ends show the painted hero, and is the time to the first painted hero on a throttled connection recorded? Quote the shot path and the time.
- Does each borrowed component or effect have its licence and last release date? Quote them.
