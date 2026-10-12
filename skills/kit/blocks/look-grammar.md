# look-grammar
Concept: The look is measured from references, written once as tokens in `DESIGN.md`, and the page code reads only those tokens.
Serves: design
Source: skills/ink/SKILL.md:23-27, :30; skills/ink/references/references.md:79, :89, :94-95, :113-143 (grammar template), :147-155 (evidence); skills/ink/references/site.md:42-53, :58-59; impeccable pack: plugin/skills/impeccable/SKILL.md:35-40 (modes); taste-skill pack: skills/taste-skill/SKILL.md:25-26, :38-39, :186, :217, :236, :531-535, :601, :687

## Rules
- Before code, write one line: "Reading this as: <page kind> for <audience>, with a <vibe> language, leaning toward <aesthetic family>."
- A tool's landing page is Persuade mode: the visitor decides and acts. Choose the mode from the surface, not the product.
- Match a look only from pixels you inspected. If the brief has no references, find them from the brief first.
- Mark each fact `visible`, `stated`, `inferred`, or `unknown`. Never upgrade `stated` or `inferred` to `visible`.
- Measure, do not only describe. A number measured from a reference becomes a rule in the grammar.
- Give colors roles, not samples: background, dominant fill, secondary fill, highlight, shadow, contour, accent.
- Borrow techniques and high-level traits. Never copy logos, exact compositions, or a living artist's exact style.
- Write `DESIGN.md` beside the page code: colors, typography, spacing, rounded, and components as YAML tokens, then prose. Lint it with `npx @google/design.md lint DESIGN.md`. It exits 0 and gives low contrast only as a warning, so read the findings. Any error or `contrast-ratio` finding is a `miss`.
- `DESIGN.md` owns color, type, and spacing. `02-design.md` keeps only what `DESIGN.md` cannot hold, such as the motion grammar, and names `DESIGN.md` for the rest. Each value has one home.
- Color literals live only in the `:root` token block, which starts a line, and in one dark-mode `:root` that is all of `@media (prefers-color-scheme: dark)`. All other CSS and page code read the tokens as custom properties.
- One accent color, saturation below 80%, used the same on the whole page. One corner-radius scale for the page. No pure black `#000000`.
- Do not default to AI-purple gradients, a centered hero over a dark mesh, three equal feature cards, or Inter + slate-900.
- Design light and dark modes from the start. Respect `prefers-color-scheme`, and put the dark tokens in that second `:root`.
- The hero fits the first viewport: headline 2 lines at most on desktop, subtext 20 words at most, the call to action visible with no scroll.
- Zero em-dashes (`—`) and en-dash separators (`–`) in visible text.

## Example
Initial state: the user links the `htmx.org` home page as the reference.
Action: write the design read: "Reading this as: dev-tool landing page for backend engineers, with a plain technical language, leaning toward minimal document style." Measure: body 18 px, max width 65ch, one accent `#3b5bdb`. Write `{{site}}/DESIGN.md` with those tokens and lint it.
Result: lint shows one `contrast-ratio` warning on the muted text, so the grammar marks it `miss` and darkens the token before style frames.

## Check (script)
- `npx @google/design.md lint {{site}}/DESIGN.md | jq -e '([.findings[] | select(.rule == "token-summary")] | length > 0) and ([.findings[] | select(.severity == "error" or .rule == "contrast-ratio")] | length == 0)'` prints `true`: `DESIGN.md` exists, its tokens parse, and lint has no error and no contrast finding. A missing file prints nothing and a file that does not parse prints `false`.
- `rg -U -n --pcre2 --glob '*.css' '^:root\s*\{[^{}]*\}(*SKIP)(*F)|@media\s*\(\s*prefers-color-scheme\s*:\s*dark\s*\)\s*\{\s*:root\s*\{[^{}]*\}\s*\}(*SKIP)(*F)|#[0-9a-fA-F]{3,8}\b(?=[^{};]*[;}])|\b(rgba?|hsla?|hwb|lab|lch|oklab|oklch|color)\(' {{site}}` prints nothing: the CSS reads only tokens. It skips a `:root` block that starts a line and a `:root` that is all of `@media (prefers-color-scheme: dark)`, so a `:root` elsewhere, such as in another media query, is a hit. A hex counts only when the first `;`, `{`, or `}` after it is `;` or `}`, so it sits in a declaration value (also before a nested rule), not in a selector such as `#face {`.
- `rg -c -i '—|–|&[mn]dash;|&#0*(821[12]|x0*201[34]);' {{site}}/index.html` prints nothing: visible text has no dash separators, as a character or an entity.
- `rg -n -g '*.js' -g '*.mjs' -g '*.ts' -g '*.jsx' -g '*.tsx' '\b(rgba?|hsla?|hwb|lab|lch|oklab|oklch)\(' {{site}}` prints nothing: page code calls no color function. A quoted hex can be an id selector such as `"#feed"`, so the judge line asks about it.

## Check (judge)
- Does `02-design.md` have the design-read line and a reference list with an evidence label on each fact? Quote the line and the labels.
- Do the HTML's `<style>` blocks and `style=` attributes, the CSS outside the two `:root` token blocks, and the page code (JS and TS) use no named color (`black`, `white`, `red`), quoted hex color in page code (`"#ff0000"`, not an id selector such as `"#feed"`), or other color literal? Quote each color value found.
- Does the hero screenshot at 1440 px show the call to action with no scroll? Quote the screenshot path.
