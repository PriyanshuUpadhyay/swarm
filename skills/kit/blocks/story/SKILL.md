---
name: story
description: "A brief and its facts become a story sheet, a structure, a shot list with key frames, and pacing that gives each beat time to read."
license: MIT
metadata:
  kind: step
  owns: "the order, the beats, the key frames, and the pacing of what the viewer sees"
  serves: "design, story"
  needs: "a brief and its facts"
  gives: "a story sheet and a shot list"
  conflicts: ""
  users: "ink, reference-analysis"
  source: "skills/ink/references/story-and-motion.md (story sheet, structures, shot list, pacing; moved 2026-10-13); skills/ink/references/qa.md (story checks: key frame, pacing)"
---

## Rules
Story sheet
- Put the story sheet at the top of the step's own file:
  `Logline:` one sentence: who or what, what changes, how it ends.
  `Structure:` linear, loop, journey, vignette, or branching.
  `Entities:` characters, props, places, each with the states it passes through.
  `Beats:` 3-8 rows: beat, intent (what the viewer should notice or feel), change of state, words on screen.
- Make each entity a function or component with parameters (pose, expression, state, seed), not a one-off drawing. The same character then appears in many shots and states without new code.
Structures
- Linear: beats in order, then an end. Typical transitions: cut, match cut, push-in. Fits an explainer, a short, a title sequence.
- Loop: the last frame returns to the first. Typical transitions: state return, self-similarity. Fits a reel background or an ambient loop.
- Journey: one continuous camera travels through places. Typical transitions: camera move, zoom-through, parallax. Fits "a day in", maps, an infinite zoom.
- Vignette: separate scenes share a motif. Typical transitions: a cut on the motif, a crossfade. Fits a montage or an anthology.
- Branching: viewer input picks the next beat. Any transition, driven by a state machine. Fits a scroll-driven or click-driven story.
Shot list
- A shot is one continuous camera view of one scene. A beat can span several shots.
- Columns: shot, beat, time or t, scene, focal subject, camera, transition in (story reason), exit → entry vector, local motion, verification frame.
- The vector column records axis, direction, and speed on both sides of a continuity cut. Write "jump" for a cut that breaks continuity on purpose.
- Mark each beat's key frame: the frame that must read on its own if the viewer sees nothing else.
Pacing
- Give each beat time to read. As starting values to tune by eye: a new subject holds about 1-2 s before it changes, and on-screen text holds about 1 s plus 0.3 s per word. A key frame that reads only when paused is too fast.
- Hold still for 0.3-0.75 s between a major action and its result. A cut straight from action to result loses the dramatic pause.
- Do not fill spare shot time with idle loops (float, breathe, pulse) as the main motion. Add story or shorten the shot. Small living motion on a character, such as a blink, is fine. HyperFrames bans idle loops outright, but its rule targets explainer videos.
- Fast action, held meaning. Show the cause, then the reaction, and give the last shot time to land. The most common viewer complaint about code-drawn reels is that they move too fast.

## Example
Initial state: the brief is "a paper boat survives a storm".
Action: write the sheet. Logline: "A paper boat rides out a storm and sails into the sunrise." Structure: linear. Entities: boat (dry, soaked, torn sail), sea (calm, storm). Five beats, each with its intent. The shot list gives shot `storm` a whip pan in, with the vector "x, right, fast" on both sides, and marks the boat on the wave crest as beat 3's key frame.
Result: the beat 3 key frame shows the boat and the storm with no motion, so it reads on its own, and the caption "Hold on" holds 1.3 s.

## Check (script)
- none: the story sheet and the shot list live in the step file, which a script check does not read.

## Check (judge)
- Does the story sheet have a logline, a structure, entities with their states, and 3-8 beats, each with its intent? Quote them.
- Does each beat name a key frame, and does that frame read on its own when paused? Quote each key frame and its path.
- Does each line of on-screen text hold about 1 s plus 0.3 s per word at playback speed? Quote the line and its hold time.
