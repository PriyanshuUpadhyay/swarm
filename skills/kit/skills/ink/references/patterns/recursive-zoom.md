# Pattern: recursive zoom

Use this when the story nests worlds inside worlds: an infinite zoom, a portal into an eye or a
window, or a journey that must loop by arriving where it began. Skip it for stories that cut between
places. A cut, a match cut, or a camera move tells those better.

## Structure

Scenes alternate in a ring, for example creature → pupil → tunnel → far end → creature. Each scene
holds one portal: a clip shape plus a child transform, `child(z) = p + k·rot(r)·z`. One loop is the
map from the root to the next copy of the root. With two scenes per loop it is `z → a·z + b`, with
`a = k₀·k₁·e^{i(r₀+r₁)}`.

## Camera

Ride the log spiral about the fixed point of that map:

```text
z* = b / (1 − a)
C_t(z) = z* + a^t · (z − z*)        world point shown at view point z, t in [0,1]
a^t = |a|^t · e^{i·t·(r₀+r₁)}       use the summed rotation, not the principal angle of a
```

Frame `t=1` then shows the copy exactly as frame `t=0` shows the root. The zoom speed is constant in
log scale, and the zoom centre stays still on screen.

## Rules that keep the seam exact

- The portal that closes the loop must hold the whole view at `t=1`: its radius, less half its rim
  stroke, is larger than the view's half-diagonal. Cap the view half-diagonal for wide aspect ratios.
- Local time per nesting level is `tau = t − depth / levelsPerLoop`. Every visible level at `t=1`
  then has the same `tau` as the matching level at `t=0`, so blinks, boil, and spin match across the
  seam, while nested copies stay out of step with each other.
- Draw nothing over a portal except its rim. Parent content, then clip, then child, then restore,
  then rim.
- Before drawing, walk down from the root while a portal (at about 90% radius) holds all four screen
  corners, and start drawing there. This works only because of the rule above.
- Stop recursing when a portal is under about 2 px and fill it with the child's main colour.
- Seed decor per scene type, not per level, or the copies differ and the seam breaks.

## Animatic

Prove nesting, camera continuity, and the seam with flat shapes first. A particle flow field is the
wrong first prototype. Check `t=0` against `t=1` and run the pops check in [qa.md](../qa.md); the
ancestor skip shows up there as a pop if a rule above is broken.

This pattern sets the structure only. The look comes from the references, built with
[mark-making.md](../mark-making.md).
