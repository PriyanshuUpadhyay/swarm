---
status: accepted
date: 2026-11-26
deciders: [user]
related: ["0094", "0167"]
informed-by:
  - 'owner, 2026-11-26: "looks pale. let’s add some sweet ass icons" (the picker board).'
  - 'owner, 2026-11-26: "It looks good, approved." (round 21, choice B: a product mark for each CLI).'
---

# 0171. A CLI shows as its product mark, with a letter box when it has none

## Context and Problem Statement

ADR 0094 draws mascots for the HUD, not vendor marks. The picker (ADR 0167) and the run chip need
to tell the CLIs apart: Claude, Codex, Antigravity and Pi. A mark of a CLI is a trademark of its
vendor. The icon packages are MIT for their files, and that does not cover the brand rules of the
vendor.

## Decision Outcome

- Marks come from the packages `simple-icons` and `@lobehub/icons-static-svg`, and only to name
  the CLI. A mark has one colour: muted, and the text colour when selected. Nobody draws a mark
  by hand.
- Only the SVG files in use ship. Each file is copied; the package is not a dependency of the app.
  The licences of the packages go in `web/NOTICE`.
- A CLI with no mark gets the letter box: 16 px, with an 11 px mono letter.
- A mark whose vendor page forbids this use becomes the letter box.

### Vendor pages read on 2026-11-26

| Vendor | URL | Read | What the page says for this use |
|---|---|---|---|
| OpenAI (Codex) | https://openai.com/brand/ | read | "Don't: Use the logo without permission or outside OpenAI's terms." Permission requests go to OpenAI. |
| Anthropic (Claude) | https://www.anthropic.com/legal/trademark-guidelines | not read | The page loaded, but the tool that fetched it showed no text on the use of a mark. |
| Google (Antigravity) | https://partnermarketinghub.withgoogle.com/brands/google/overview/ | read | It gives no public rule for a third-party app. Its full rules need a partner sign-in. |
| Pi project | https://pi.dev | read | The page shows no brand or trademark rule. |

The reading of 2026-11-26 is not complete. The OpenAI page asks for permission for a use of its logo.
The Anthropic page was not read. The Google page gives its full rules only to partners. The Pi page has no
brand rule. Run 4 reads each rule again before a mark ships. The owner decides each mark whose page asks
for permission or cannot be read. Until that decision, such a mark shows as the letter box.

## Consequences

- Good, because the app never ships a mark that a vendor forbids.
- Bad, because one or more CLIs may show a letter box until the owner decides.
- Bad, because vendor rules change. The date above is the date of the reading.
- The picker and the marks are built in run 4.
