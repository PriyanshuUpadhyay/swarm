# html-diagram
Concept: Draw a tech diagram with HTML and CSS boxes that reflow, so labels never clip or cross at any width.
Serves: design, build
Source: impeccable pack: plugin/skills/impeccable/reference/adapt.md:140 (grid and flexbox reflow), reference/layout.md:52 (structural reflow); blocks/web-ui-basics.md:10 (a diagram needs a text description)

## Rules
- Build each node as an HTML element with real text, laid out with CSS grid or flexbox. Draw arrows with CSS borders, pseudo-elements, or text arrows (→ ↓), never with absolute pixel positions.
- At 600 px and below, switch the diagram to one column in reading order. Each arrow keeps its meaning, source to target: it points at the same box as on a wide screen, so a stacked arrow never points away from its target. Turn it to match the new layout (↓ when the target is now below, ↑ when it is above).
- Wrap each diagram in `<figure>` with a `<figcaption>`. Give the diagram `role="img"` and an `aria-label` that says what it shows, or use a list (`<ol>`) when the order is the point.
- Use the page fonts and design tokens. Label text is 13 px or larger at every width.
- Use no SVG coordinates, no canvas, and no diagram library.
- Limit: curved or crossing arrows are hard in CSS. If the shape needs them, the shape is too complex; split the diagram in two.

## Example
Initial state: the concept block chose a 4-step flow, chair → swarm → worker → inbox, so the 2-to-3-column limit for a parts diagram does not apply. Action: one `<ol class="flow">` with 4 `<li>` boxes in a CSS grid of 4 columns, arrows as `::after` with "→", and one column with "↓" under 600 px, because each target now sits below its source. Result: at 375 px the 4 boxes stack, each arrow still points from source to target, and no label clips.

## Check (script)
- `rg -c -g '*.html' '<figure(\s|>|$)' {{site}}` prints a count of 1 or more, and `rg -U --pcre2 -n -g '*.html' '<figure[\s>](?:(?!</figure>|<figcaption)[\s\S])*</figure>|<figure[\s>](?:(?!</figure>)[\s\S])*?<figcaption(?:(?!</figure>)[\s\S])*?<figcaption' {{site}}` prints nothing and exits 1: the page has a figure, and each figure has exactly one caption.

## Check (judge)
- Does any diagram draw with SVG, canvas, or absolute pixel positions? Quote each diagram's markup and the CSS that places its arrows.
- In each 375 px shot, is any diagram wider than its column, cut off or scrolled sideways? Quote the shot path.
- At 375 px, does each arrow still point from its source to its target? Quote the screenshot path and, for each arrow, its source box, its target box, and its direction.
