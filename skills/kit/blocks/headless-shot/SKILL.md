# headless-shot
Concept: Take page screenshots with Playwright's chrome-headless-shell, and know its limits.
Serves: design, build, review
Source: skills/ink/references/build.md:156-158 (chrome-headless-shell, Playwright cache); skills/ink/references/qa.md:30-38 (compare image at the same scale, 100 % crop)

## Rules
- Command: `mkdir -p {{shots}} && <chrome-headless-shell> --headless --screenshot="$PWD/{{shots}}/<name>-<W>.png" --window-size=W,H "file://$PWD/{{site}}/index.html"`, run from the repo root; the shell does not make the folder. Widths are 1440,900 and 390,844 and 375,812. `<name>` is the page, such as `home`, or `diagram-<diagram>` for a shot of one diagram.
- For a reduced-motion shot, add `--force-prefers-reduced-motion` to the command and name it `{{shots}}/<name>-<W>-reduced.png`. The page then matches `@media (prefers-reduced-motion: reduce)`.
- Find `<chrome-headless-shell>` with `npx playwright install --dry-run chromium-headless-shell`, which prints the shell's `Install location:` folder (the first one; the second is FFmpeg) whether or not it is installed. If that folder does not exist, run the same command without `--dry-run`. The binary is `chrome-headless-shell` in the folder's `chrome-headless-shell-*` subfolder.
- Put a frame beside the reference at the same scale with `magick ref.png frame.png -resize x900 +append {{shots}}/compare-NN.png`.
- Limit: the screenshot is one viewport tall. For a part lower on a long page, shoot with a taller window (for example 375,4000), then cut the part out at 100 % with `magick tall.png -crop 375x812+0+Y +repage {{shots}}/<name>-375.png`, where Y is the part's top in the tall shot. Do not use an anchor URL (`index.html#id`): this binary gives an all-white shot for it.
- Limit: an image viewer may show a 375 px shot downscaled. Crop the part you judge at 100 % (`magick out.png -crop 375x400+0+0 crop.png`) before you judge small text.
- Limit: it runs no real clicks or clipboard. Say "not tested in a real browser" for those.
- Use this binary, not a desktop Chrome app.

## Example
Initial state: `{{site}}/index.html` exists. Action: shoot 1440,900 and 375,812 to `{{shots}}/home-1440.png` and `{{shots}}/home-375.png`, then crop the 375 shot's diagram at 100 %. Result: the crop shows label size truly, and the report names the clipboard as not tested.

## Check (script)
- `file {{shots}}/*.png` prints `PNG image data, W x H` for each shot, and each W other than a compare image's is 1440, 390, or 375: each shot is a real screenshot at a check width.
- `magick {{shots}}/*.png -format '%f %[fx:standard_deviation]\n' info:` prints a value above 0 for each shot: no shot is one flat color, as a blank shot is.

## Check (judge)
- Does each screenshot path the step names exist in {{shots}}? Quote each path.
- Does each claim about small text cite a 100 % crop, not a full downscaled shot? Quote the claim and the crop path.
- Are untested interactions (real clicks, clipboard) named as not tested in a real browser? Quote the line.
