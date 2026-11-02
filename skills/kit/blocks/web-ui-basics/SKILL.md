---
name: web-ui-basics
description: "Check the floor every static one-page site must meet: structure, focus, contrast, layout at each width, and motion. Use when you build or review a static web page."
license: MIT
metadata:
  kind: rule
  owns: "the floor for a static one-page site"
  serves: "build"
  needs: "a page"
  gives: "pass or fail per check"
  conflicts: ""
  users: "implement, build, build-site"
  source: "web-interface-guidelines pack: command.md:16-26, :31-35, :53-60, :64, :80, :119-120, :143-146, :153; skills/ink/references/site.md (moved to blocks/build-site 2026-10-13); skills/ink/references/qa.md (scroll story check, hidden content); skills/ink/references/story-and-motion.md (Scroll driver); taste-skill pack: skills/taste-skill/SKILL.md:153-154, :225, :260, :522, :529; emilkowalski pack: skills/emil-design-eng/SKILL.md:95-135, :527-531; blocks/headless-shot/SKILL.md:17 (375 px), :18 (reduced-motion shot)"
---

## Rules
Structure
- Semantic HTML before ARIA. `<button>` for actions, `<a>` for navigation, never a `<div>` with a click handler.
- Headings go in order, `<h1>` to `<h6>`. Add a skip link to the main content. Heading anchors get `scroll-margin-top`.
- Images need `alt` (`alt=""` if decorative) and explicit `width` and `height`. Decorative icons get `aria-hidden="true"`. Icon-only buttons get `aria-label`. For diagrams, apply `blocks/diagram/SKILL.md`.
Focus
- Every link and control takes focus with Tab and shows a ring. Use `:focus-visible`. Never `outline: none` without a replacement. A sticky header must not cover the focused element.
Contrast
- Text meets WCAG AA: 4.5:1 for body, 3:1 for large text (24 px and up, or 18.7 px and up when bold). Text over an image, a video, or a 3D scene passes in the screenshot, not only in `DESIGN.md`.
Layout
- Check widths are 1440 px, 390 px, and 375 px.
- No sideways scroll at any check width: `document.documentElement.scrollWidth <= innerWidth`.
- Each multi-column layout declares its below-768 px fallback. Use CSS Grid, not flex percentage math. A stable full-height hero uses `min-height: 100svh`. UI that tracks the visible area uses `100dvh`.
- Never disable zoom (`user-scalable=no`, `maximum-scale=1`). Set `color-scheme` and a `<meta name="theme-color">` that matches the background.
Motion
- UI timing, easing, and reduced motion follow the kit block `blocks/ui-motion/SKILL.md`, including its checks.
- Every key fact shows in a settled state. Content that waits at opacity 0 is blank in full-page screenshots and skipped by checkers. Capture that state by `blocks/headless-shot/SKILL.md`.
Copy
- `…` not `...`. Active voice. Specific button labels: "Copy install command", not "Continue".

## Example
Initial state: the hero `h1`, three sections, and a copy button exist.
Action: write and run `checks/basics.mjs` at 1440, 390, and 375 px. It reports `scrollWidth 412 > 375` from a diagram. Make the diagram fit its column at that width. Run again: all widths pass.
Result: the evidence line shows the FAIL and then the PASS for each width.

## Check (script)
- `rg -n -g '*.html' -g '*.css' -g '*.js' 'user-scalable=no|maximum-scale=1(\.0+)?([^.0-9]|$)' {{site}}` prints nothing: no page turns off zoom.
- `! rg -q -g '*.html' -g '*.css' -g '*.js' 'outline:\s*none' {{site}} || rg -q -g '*.html' -g '*.css' -g '*.js' ':focus-visible' {{site}}` exits 0: an `outline: none` comes with a `:focus-visible` replacement.
- `test -f {{site}}/index.html && rg -o -N '<h[1-6]' {{site}}/index.html | tr -d '<h' | awk '$1 > p + 1 {bad = 1} {p = $1} END {exit bad}'` exits 0: the headings start at `h1` and skip no level.
- `rg --files -0 -g '*.html' {{site}} | xargs -0 perl -0777 -ne 'while (/<img\b[^>]*>/g) { my $t = $&; print "$ARGV: $t\n" unless $t =~ /\balt=/ && $t =~ /\bwidth=/ && $t =~ /\bheight=/ }'` prints nothing: each image tag, also one split over lines, has `alt`, `width`, and `height`.

## Check (judge)
- Do the hits of `rg -n -g '*.html' -g '*.css' -g '*.js' 'transition:\s*all|ease-in([^-]|$)|prefers-reduced-motion' {{site}}` pass the judge in `blocks/ui-motion/SKILL.md`? Quote each hit and its verdict.
- In the shots at 1440, 390, and 375 px, is any content cut off at the right edge or scrolled sideways? Quote each shot path.
- Does the settled-state shot show every key fact? Quote the path and the facts. Apply `blocks/ui-motion/SKILL.md` for reduced motion.
- Does a Tab walk show a visible ring on each link and button, with the skip link first? Quote the screenshot paths.
- For diagrams, apply the judge checks in `blocks/diagram/SKILL.md`.
