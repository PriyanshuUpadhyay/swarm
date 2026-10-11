# Story, storyboard, and motion

## Contents

- Story sheet
- Structures
- Drivers
- Shot list
- Transition catalogue
- Seam law (continuity cuts)
- Scene graph
- Timeline
- Pacing
- Motion feel
- Web-native media ideas
- Seam design (loops only)

## Story sheet

Put this at the top of `ink/03-story.md`:

```text
Logline:    one sentence: who or what, what changes, how it ends
Structure:  linear | loop | journey | vignette | branching
Entities:   characters, props, places, each with the states it passes through
Beats:      3-8 rows: beat, intent (what the viewer should notice or feel), change of state, words on screen
```

Make each entity a draw function with parameters (pose, expression, state, seed), not a one-off
drawing. The same character then appears in many shots and states without new code.

## Structures

| Structure | Shape | Typical transitions | Fits |
|---|---|---|---|
| Linear | beats in order, then an end | cut, match cut, push-in | explainer, short, title sequence |
| Loop | last frame returns to the first | state return, self-similarity | reel background, ambient loop |
| Journey | one continuous camera travels through places | camera move, zoom-through, parallax | "a day in", maps, infinite zoom |
| Vignette | separate scenes share a motif | cut on the motif, crossfade | montage, anthology |
| Branching | viewer input picks the next beat | any, driven by a state machine | scroll-driven or click-driven story |

## Drivers

What moves the story forward decides how time works in the build.

| Driver | Time source | Fits |
|---|---|---|
| Playback | one master clock | video, loop, reel |
| Scroll | the reader's scroll progress, per section | essay, explainer, editorial story |
| Gates | a reader action ends a segment | interactive and branching stories |

### Scroll

- Map each section's scroll progress to that section's shot-local `u`. For DOM layers, CSS
  scroll-driven animations (`animation-timeline: view()` or `scroll()`) run off the main thread;
  NRK measured 1 ms against 0.16 ms per task. For a canvas, read the progress and render that `u`.
- Put every key fact in a settled state that the scroll can rest on, not only mid-motion. Wheel
  notches and the keyboard scroll in steps and jump over frames.
- Treat motion as a progressive enhancement. Under `prefers-reduced-motion: reduce`, show each
  beat's key frame as a still.

Source: [NRK case study](https://developer.chrome.com/blog/nrk-casestudy), 2025.

### Gates

A gate pauses or redirects the flow until the reader acts. ZERO joins six scroll stages with five
gates, such as drawing a zero or holding to shatter glass
([Codrops, 2026](https://tympanus.net/codrops/2026/07/17/zero-the-engineering-behind-a-defiant-interactive-narrative/)).

- For each gate, define the action and its visible hint, the state it changes (branch, text, look,
  or only mood), the response to a failed try, and an escape (a skip control or a timeout) so no
  reader is stuck.
- Write failure paths as story, not as error messages.
- Keep gates few. Interactive-fiction players report that hard gates kill momentum.
- In the shot list, a gate is an entry with `type: 'gate'`. Time stops at the gate, and each segment
  between gates has its own `u`.

## Shot list

A shot is one continuous camera view of one scene. A beat can span several shots.

| Shot | Beat | Time / t | Scene | Focal subject | Camera | Transition in (story reason) | Exit → entry vector | Local motion | Verification frame |
|---|---|---|---|---|---|---|---|---|---|

The vector column records axis, direction, and speed on both sides of a continuity cut (see the
seam law below). Write "jump" for a cut that breaks continuity on purpose.

Mark a beat's key frame: the frame that must read on its own if the viewer sees nothing else.

## Transition catalogue

Choose each transition for what the story does at that boundary. Do not use one transition for every
boundary.

| Transition | Story meaning | Build |
|---|---|---|
| Cut | jump in time or place, energy | switch shot at a frame; pair with a visual or sound accent |
| Match cut | two things are linked | end shot A and start shot B on the same shape, position, colour, or motion |
| Crossfade | time passes, memory | draw both shots and blend over the window; it has no carrier, so do not use it for continuity |
| Wipe or iris | chapter change, playful | mask shot B with a moving shape over shot A |
| Camera move (pan, push, pull) | same time and place, reveal | animate the camera in one scene, or lay two scenes side by side in world space |
| Whip pan or blur cut | fast jump with energy | motion-blurred move out of A and into B, cut hidden at peak blur; the same peak blur on both sides, about 10 px on text and 18-20 px on a full frame |
| Zoom-through (portal) | a world inside a world | nested transforms and a clip; see [recursive-zoom.md](patterns/recursive-zoom.md) |
| Morph | one thing becomes another | interpolate shared control points; plan matching point counts early |
| Occlusion | hidden cut | a foreground object fills the frame; switch scenes behind it |

Rule of thumb: continuity (same time or place) gets a camera move or match cut. A jump gets a cut or
wipe. A transformation gets a morph. Nesting gets a zoom-through.

## Seam law (continuity cuts)

A cut feels like one continuous move when the exit of shot A sets the entry of shot B:

- **Axis.** x stays x, y stays y, zoom stays zoom.
- **Direction.** Never mirror. On the zoom axis the sign matters, so a push (growing) answers a
  push and a pull (shrinking) answers a pull. A pull answered by grow-from-small is the most common
  mistake, because grow-from-small is the default entrance.
- **Speed.** Use mirrored easing, so the entry's first velocity is close to the exit's last.
- **Phase.** Cut mid-motion on both sides. A shot that settles to rest before the cut is a dead
  beat.

Pick one dominant direction, the current, for ordinary beat changes. Keep other directions for
meaning: upward for a reveal, zoom in for going deeper, zoom out for an arrival. Change direction
only with a visible cause or at a chapter break. Where you can, hand a concrete carrier across the
cut: an object, a cursor, or a shape at the same position and speed.

Source: HeyGen HyperFrames agent skills
[motion-doctrine](https://github.com/heygen-com/hyperframes/blob/main/.agents/skills/motion-doctrine/SKILL.md)
and [cut-the-curve](https://github.com/heygen-com/hyperframes/blob/main/.agents/skills/cut-the-curve/SKILL.md),
which also keep every cut in a ledger file and check it with a script.

## Scene graph

Show nesting and compositing explicitly in `ink/03-story.md`:

```text
Root
├── shot timeline: active shot, or two shots inside a transition window
│   ├── scene (own coordinates, shot-local time u)
│   │   ├── background
│   │   ├── entities with parameters
│   │   └── optional mask or portal
│   │       └── nested scene
│   └── incoming scene (only inside a transition window)
├── texture overlays (screen space)
├── lettering and captions
└── debug overlays: shot id, beat, safe area, masks
```

For each node record the parent, local transform, draw order, mask or clip, lifetime in shots, local
animation, and whether it uses world, camera, or screen coordinates.

## Timeline

Drive a playback story from one master time. A scroll or gate story gives each section or segment
its own time instead. Either way, derive shot-local time from the shot list, never inside scene code:

```js
const shots = [
  { id: 'calm',  scene: 'sea', dur: 4, in: { type: 'cut' } },
  { id: 'storm', scene: 'sea', dur: 6, in: { type: 'whip', dur: 0.4 } },
  { id: 'dawn',  scene: 'sky', dur: 5, in: { type: 'match', dur: 0.5 } },
];
// shotAt(time) -> { shot, u in [0,1), incoming?, k in [0,1] }
```

Keep helpers for clamping and easing. Use explicit keyframes for camera travel and deterministic
procedural motion for secondary elements.

When one entity appears in consecutive shots, key its secondary motion (blink, sway, flicker) to the
master time, not to shot-local time, so it does not restart at every cut.

## Pacing

Give each beat time to read. As starting values to tune by eye: a new subject holds about 1-2 s
before it changes, and on-screen text holds about 1 s plus 0.3 s per word. A key frame that reads
only when paused is too fast.

- Hold still for 0.3-0.75 s between a major action and its result. A cut straight from action to
  result loses the dramatic pause.
- Do not fill spare shot time with idle loops (float, breathe, pulse) as the main motion. Add story
  or shorten the shot. Small living motion on a character, such as a blink, is fine. HyperFrames
  bans idle loops outright, but its rule targets explainer videos.
- Fast action, held meaning. Show the cause, then the reaction, and give the last shot time to land.
  The most common viewer complaint about code-drawn reels is that they move too fast.

## Motion feel

Smooth 60 fps interpolation of everything reads as software, not as a drawn film.

- **Boil.** Hand-drawn lines change 6-12 times per second, not every frame. At 30 fps use
  `boil = Math.floor(frame / 3)` or `/ 4`, and seed each element's wobble with `hash(key, boil)`.
  Boil characters and hand-drawn props only. Text, halftone, captions, interface layers, and a
  static page do not boil. Keep hatching in the element's local space so it moves with the element.
- **On twos.** Character poses may hold for two frames (12 poses per second) while the camera moves
  on every frame.
- **Move, then rest.** Put a motion in the first half of its period (about 55 %) and hold for the
  rest. Motion that never settles reads as mechanical.
- **Staged arrival.** Layers land one after another, not all at once. In one reel the three inks
  land at 0.3, 1.1, and 1.9 s, and the plate offset settles from 14 px to 4 px over 2.7 s.
- **Draw-in.** Reveal a stroke with a length budget from 0 to 1 instead of a fade.
- **No per-frame randomness.** Grain re-rolls on a slower clock (every 2 frames, or a third of the
  specks), and plate offsets stay fixed for a shot.

Sources: iart-ai `javascript-animation-skills`, lemo-opuscar `styles/*/STYLE.md`, ClaudeAnimationBase
`ANIMATION_GUIDE.md`, Glitch Cat Club `insta-glitch/artefact/reel.py`, and Jon Tirudd on
[animation boil](https://www.jontirudd.com/post/animation-boil).

## Web-native media ideas

The browser itself can carry the story. Write a pattern file when a build uses one of these.

| Medium | How | Example | Weakness |
|---|---|---|---|
| Several windows as one stage | sync state through `localStorage` events and `window.screenX/Y` | [multipleWindow3dScene](https://github.com/bgstaal/multipleWindow3dScene) | desktop only |
| Text-mode look | render the scene as a glyph grid | [textmode.js](https://code.textmode.art), [play.ertdfgcvb.xyz](https://play.ertdfgcvb.xyz) | a look, not a story layer |
| Card stack | HyperCard-like cards with scripts | [Decker](https://github.com/JohnEarnest/Decker) | fixed 1-bit look |
| Community-voted chapters | shared votes open the next chapter | [The Haunted Thread](https://developers.reddit.com/apps/chainstory130) | needs shared server state |
| Live model on a fixed spine | a language model improvises scenes but steers back to fixed beats | [The Long Context](https://thelongcontext.com) | one project; uneven prose |

## Seam design (loops only)

Choose one:

- exact state return at `t=1`;
- cyclic functions with integer phase counts;
- self-similar nesting, where `t=1` shows a nested copy framed like the root (see the pattern file);
- matched start and end compositions joined in the editor;
- a designed wipe or occlusion that hides the seam.

Verify frames just before and after the seam, not only the first and last frame.
