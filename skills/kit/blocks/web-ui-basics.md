# web-ui-basics
Concept: The floor every static one-page site must meet: structure, focus, contrast, layout at each width, and motion.
Serves: build
Source: web-interface-guidelines pack: command.md:16-26, :31-35, :53-60, :64, :80, :119-120, :143-146, :153; skills/ink/references/site.md:25, :85-88; skills/ink/references/qa.md:57-58, :110-111; skills/ink/references/story-and-motion.md:59-60; taste-skill pack: skills/taste-skill/SKILL.md:153-154, :225, :260, :522, :529; emilkowalski pack: skills/emil-design-eng/SKILL.md:95-135, :527-531; blocks/headless-shot.md:7 (375 px), :8 (reduced-motion shot)

## Rules
Structure
- Semantic HTML before ARIA. `<button>` for actions, `<a>` for navigation, never a `<div>` with a click handler.
- Headings go in order, `<h1>` to `<h6>`. Add a skip link to the main content. Heading anchors get `scroll-margin-top`.
- Images need `alt` (`alt=""` if decorative) and explicit `width` and `height`. Decorative icons get `aria-hidden="true"`. Icon-only buttons get `aria-label`. Meaningful media, such as a diagram, needs a text description.
Focus
- Every link and control takes focus with Tab and shows a ring. Use `:focus-visible`. Never `outline: none` without a replacement. A sticky header must not cover the focused element.
Contrast
- Text meets WCAG AA: 4.5:1 for body, 3:1 for large text (24 px and up, or 18.7 px and up when bold). Text over an image passes in the screenshot, not only in `DESIGN.md`.
Layout
- Check widths are 1440 px, 390 px, and 375 px.
- No sideways scroll at any check width: `document.documentElement.scrollWidth <= innerWidth`.
- Each multi-column layout declares its below-768 px fallback. Use CSS Grid, not flex percentage math. A full-height hero uses `min-height: 100dvh`, not `100vh`.
- Never disable zoom (`user-scalable=no`, `maximum-scale=1`). Set `color-scheme` and a `<meta name="theme-color">` that matches the background.
Motion
- Animate only `transform` and `opacity`. Never `transition: all`.
- UI animations stay under 300 ms. Use ease-out to enter. Never ease-in.
- Honor `prefers-reduced-motion: reduce`: fewer and gentler, not zero. Keep opacity fades; remove movement. Loops and parallax become static.
- Every key fact shows in a settled state and under reduced motion. Content that waits at opacity 0 is blank in full-page screenshots and skipped by checkers, so check with reduced motion on: take the reduced-motion shot in blocks/headless-shot.md:8.
Copy
- `…` not `...`. Active voice. Specific button labels: "Copy install command", not "Continue".

## Example
Initial state: the hero `h1`, three sections, and a copy button exist.
Action: write and run `checks/basics.mjs` at 1440, 390, and 375 px. It reports `scrollWidth 412 > 375` from a diagram. Make the diagram fit its column at that width. Run again: all widths pass.
Result: the evidence line shows the FAIL and then the PASS for each width.

## Check (script)
- `rg -n -g '*.html' -g '*.css' -g '*.js' 'outline:\s*none|transition:\s*all|user-scalable=no|maximum-scale=1(\.0+)?([^.0-9]|$)|ease-in([^-]|$)' {{site}}` prints nothing, or each `outline: none` has a `:focus-visible` rule beside it: no banned pattern.
- `rg -n -g '*.html' -g '*.css' -g '*.js' 'prefers-reduced-motion' {{site}}` finds a block: reduced motion is handled.
- `rg -o '<h[1-6]' {{site}}/index.html` lists the headings with no skipped level: heading order holds.
- `rg -U -n -g '*.html' '<img\b[^>]*>' {{site}}` prints each whole image tag, also one split over lines, and shows `alt`, `width`, and `height` on each image: images are described and sized.

## Check (judge)
- In the shots at 1440, 390, and 375 px, is any content cut off at the right edge or scrolled sideways? Quote each shot path.
- Does the reduced-motion shot (`{{shots}}/<name>-<W>-reduced.png`, from blocks/headless-shot.md:8) show every key fact? Quote the path and the facts.
- Does a Tab walk show a visible ring on each link and button, with the skip link first? Quote the screenshot paths.
- Does each diagram have a text description? Quote it.
