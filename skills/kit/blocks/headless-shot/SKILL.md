---
name: headless-shot
description: "Take screenshots and compare images with the backend the artifact needs. Use when a step needs screenshots of a page or a drawn frame."
license: MIT
metadata:
  kind: step
  owns: "how page screenshots are taken"
  serves: "design, build, review"
  needs: "a URL or a file"
  gives: "screenshots at set widths"
  conflicts: ""
  users: "implement, build, build-site"
  source: "skills/build/references/build.md (frame export with chrome-headless-shell); skills/build/references/qa.md (compare image at the same scale, 100 % crop; moved 2026-10-13); 2026-09 macOS arm64 trial (WebGPU blank in the shell, correct in full Chrome); 2026-09 Obscura 0.2.2 trial (canvas transforms, arcs, clips, and text drawn wrong)"
---

## Rules
- Command: `mkdir -p {{shots}} && <chrome-headless-shell> --headless --screenshot="$PWD/{{shots}}/<name>-<W>.png" --window-size=W,H "file://$PWD/{{site}}/index.html"`, run from the repo root; the shell does not make the folder. For page shots, sizes are 1440,900 and 390,844 and 375,812. For drawn stills and reel frames, use the delivery size that the build step names. `<name>` is the page, such as `home`, or `diagram-<diagram>` for a shot of one diagram.
- For a reduced-motion shot, add `--force-prefers-reduced-motion` to the command and name it `{{shots}}/<name>-<W>-reduced.png`. The page then matches `@media (prefers-reduced-motion: reduce)`.
- Find `<chrome-headless-shell>` with `npx playwright install --dry-run chromium-headless-shell`, which prints the shell's `Install location:` folder (the first one; the second is FFmpeg) whether or not it is installed. If that folder does not exist, run the same command without `--dry-run`. The binary is `chrome-headless-shell` in the folder's `chrome-headless-shell-*` subfolder.
- Put a frame beside the reference at the same scale with `magick ref.png frame.png -resize x900 +append {{shots}}/compare-NN.png`.
- Limit: the screenshot is one viewport tall. For a part lower on a long page, shoot with a taller window (for example 375,4000), then cut the part out at 100 % with `magick tall.png -crop 375x812+0+Y +repage {{shots}}/<name>-375.png`, where Y is the part's top in the tall shot. Do not use an anchor URL (`index.html#id`): this binary gives an all-white shot for it.
- Limit: an image viewer may show a 375 px shot downscaled. Crop the part you judge at 100 % (`magick out.png -crop 375x400+0+0 crop.png`) before you judge small text.
- Limit: it runs no real clicks or clipboard. Say "not tested in a real browser" for those.
- Use `chrome-headless-shell` for DOM, CSS, Canvas 2D, and WebGL captures. For WebGPU, use full Chrome for Testing from Playwright's `chromium-*` folder with `--headless=new --enable-unsafe-webgpu --use-mock-keychain` and a fresh `--user-data-dir`. Check one frame before a long export.
- For WebGPU, drive Chrome for Testing over the DevTools protocol. Call `window.seek(t)` with `Runtime.evaluate` and `awaitPromise: true`, then call `Page.captureScreenshot`. The artifact's `seek(t)` must meet the readiness contract in `blocks/build-canvas/SKILL.md`; `--screenshot` cannot wait for it.
- Do not use Lightpanda or Obscura for canvas frames.
- On macOS, the fresh capture profile uses the mock keychain. If a keychain prompt still appears, Deny is safe because the profile is empty.

## Example
Initial state: `{{site}}/index.html` exists. Action: shoot 1440,900 and 375,812 to `{{shots}}/home-1440.png` and `{{shots}}/home-375.png`, then crop the 375 shot's diagram at 100 %. Result: the crop shows label size truly, and the report names the clipboard as not tested.

## Check (script)
- `file {{shots}}/*.png` prints `PNG image data, W x H` for each shot, and each page shot's W is 1440, 390, or 375. Compare images and crops are excluded. Drawn stills and reel frames match the delivery size that the build step names.
- `magick {{shots}}/*.png -format '%f %[fx:standard_deviation]\n' info:` prints a value above 0 for each shot: no shot is one flat color, as a blank shot is.

## Check (judge)
- Does each screenshot path the step names exist in {{shots}}? Quote each path.
- Does each claim about small text cite a 100 % crop, not a full downscaled shot? Quote the claim and the crop path.
- Are untested interactions (real clicks, clipboard) named as not tested in a real browser? Quote the line.
