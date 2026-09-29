# Prepend without a jump: stable reverse infinite scroll in a SwiftUI chat transcript

Date: 2026-09-29. Scope: macOS 26 SwiftUI app, Swift 6, `ScrollView` + `LazyVStack` transcript
(`ui/Sources/Swarm/Transcript/TranscriptView.swift`). Research only. No repo code changed.

## Result

Use the **inverted scroll view**. Flip the `ScrollView` upside down, flip each row back, and feed
the rows newest first. Then "load older rows" is an **append at the end of the document**. A
scroll view never moves its viewport when content grows after it, so the row the user reads stays
still, with no anchor code, no scroll-phase wait, and no offset correction.

Example (measured, probe below): the user reads row 942 near the top of the window, mid trackpad
gesture. 40 older rows arrive. Row 942 moves 0.0 pt at the insert, and the next three wheel events
move it by exactly 3 × 10 pt. With the current `.scrollPosition(id:anchor: .top)` setup, the same
row leaves the screen (it moved more than the 600 pt window height).

Rule: in a scroll view, growth *after* the visible region is free and growth *before* it must be
corrected, so turn "before" into "after".

## Ranking (simplest and most robust first)

| # | Option | Prepend while idle | Prepend mid-gesture | Scroll up into new rows (late height change) | Code | Verdict |
|---|---|---|---|---|---|---|
| 1 | **Inverted `ScrollView` + `LazyVStack`** (flip container and rows) | 0 pt (measured) | 0 pt (measured) | constant 20 pt/event (measured) | ~6 lines | **Recommended** |
| 2 | AppKit `NSTableView`/`NSScrollView` with a non-flipped document view (Telegram macOS) | 0 by geometry (see below) | not measured | not measured | large (representable, cell hosting) | Fallback if option 1 hits a blocker |
| 3 | `.scrollTargetLayout()` + `.scrollPosition(id:anchor: .top)` (current code) | ~0 pt (measured) | row leaves screen (measured) | steps of 33 and 97 pt (jitter, measured) | small | Not enough |
| 4 | `.defaultScrollAnchor(.bottom, for: .sizeChanges)` | row moved 844 pt (measured) | row moved 724 pt (measured) | – | 1 line | Does not solve prepend when not at the bottom edge |
| 5 | Non-lazy `VStack` + `.sizeChanges` bottom | row moved 4530 pt = full insert height (measured) | 4320 pt (measured) | constant (exact heights) | 1 line | No anchor at all; only removes estimate jitter |
| 6 | Hold rows until scroll phase is idle, then insert | holds (owner's probe) | deferred | jitter remains (owner's recording) | medium | Treats the symptom |
| 7 | `ScrollViewReader.scrollTo` / `ScrollPosition.scrollTo(y:)` / `NSClipView.scroll(to:)` after insert | one wrong frame (owner's probe) | ignored (owner's probe) | – | medium | Fragile; Apple says offsets in lazy stacks are estimates |
| 8 | Virtual huge content space (FluidGroup `swiftui-messaging-ui`) | no jump (library claim) | – | – | a library, `UICollectionView`, iOS only | Not available on macOS |

## Why the current setup jitters (root cause, from Apple)

WWDC26 session 321 "Dive into lazy stacks and scrolling with SwiftUI" (Rens Breur, UI Frameworks
Engineer, https://developer.apple.com/videos/play/wwdc2026/321/) states, quoted from the transcript:

- "Since a LazyVStack doesn't load all of its views, the height of the subviews that are off-screen
  are estimated. This estimated height is based on the average size of views that have been placed
  before".
- "The space above the visible rect isn't precise either. The scroll position, or content offset of
  the scroll view, therefore depends on an estimated position of the visible items."
- "The lazy stack and the embedding scroll view coordinate the position and content offset. That
  way, when the estimations are updated, the relative position of the visible subviews in the
  scroll view doesn't change."
- "avoid using the absolute content size or content offset with lazy stacks, since these are
  estimated and unstable."
- "don't change the layout of subviews of lazy stacks after they appear, as that can push the lazy
  stack out of the targeted scroll position." And: "The lazy stack measures the view's original
  height, but the height changes after the view appears, pushing down other content."

So SwiftUI corrects *estimates* for you, but not a row that **changes its own height after it
appears**. The Swarm transcript has such rows:

- `ui/Sources/Swarm/Transcript/TranscriptMessageView.swift:27-42`: a message longer than 4096
  characters first shows a plain-text prefix, then swaps in parsed blocks from a `.task`.
- `ui/Sources/Swarm/Transcript/TranscriptBoundedTextView.swift:53-72`: a fixed-height placeholder,
  then chunks from a `.task` (height is fixed here, so this one is likely safe).
- `ui/Sources/Swarm/Transcript/TranscriptToolCard.swift:189, 214, 311, 330`: previews prepared in
  `.task` after appear.
- `TranscriptView.swift:136-170`: the "Load earlier messages" button label and the
  "Show N hidden rows" button sit *above* the first row; their text and presence change when older
  rows load (unverified whether this adds visible motion).

The probe reproduces this: with rows that grow 30 ms after appear (`LATE=1`), scrolling up into
new rows with `.scrollPosition(id:anchor: .top)` gives per-event steps
`[20, 20, 20, 20, 33, 20, …, 20, 97, 20, …]`, so the view jumps by 13 and 77 pt. In the inverted
list, the same late growth happens at the document end (visually above the reader), and the steps
stay `[20, 20, …, 20]`.

## Recommended approach: inverted list

```swift
extension View {
    /// Mirror vertically. Rotation by pi plus an x-mirror is what Stream Chat uses;
    /// `.scaleEffect(x: 1, y: -1)` is the same transform.
    func upsideDown() -> some View {
        rotationEffect(.radians(.pi)).scaleEffect(x: -1, y: 1, anchor: .center)
    }
}

ScrollView {
    LazyVStack(alignment: .leading, spacing: DesignTokens.Spacing.m) {
        ForEach(rowsNewestFirst) { row in          // newest row is first in the document
            TranscriptRowView(row: row, …).upsideDown()
        }
        if hasOlder {                               // sentinel at the document end = visual top
            ProgressView().upsideDown()
                .onAppear { startLoadingOlder() }   // or onScrollTargetVisibilityChange
        }
    }
    .scrollTargetLayout()
}
.upsideDown()
// Edges swap: the composer is at the logical .top now.
.contentMargins(.top, composerHeight + DesignTokens.Spacing.l, for: .scrollContent)
```

What it removes from `TranscriptView.swift`: the `.scrollPosition(id: $topRowID, anchor: .top)`
hold, `.defaultScrollAnchor(.bottom, for: .initialOffset)` (offset 0 is already the newest row),
the held-snapshot wait on scroll phase, and most of `followLatest`. The probe shows that when the
reader is at the newest edge (offset 0) and the newest row grows by 300 pt, its bottom edge moves
0 pt, so streaming output stays in view without a scroll call. When the reader is in history and
the newest row grows, the reader's row moves 0 pt.

Loading trigger: WWDC26 session 321 and the WWDC26 SwiftUI group lab
(https://developer.apple.com/videos/play/wwdc2026/8006/) both suggest an `onAppear` on a last
view, or `onScrollTargetVisibilityChange`, rather than reading the absolute content offset.

### Measured results (probe, macOS 27.0 build 26A428, Swift 6.4, synthetic wheel events)

| Mode | wheel +30 | idle prepend | mid-gesture prepend | steps into new rows | late-height steps (`LATE=1`) | newest row +300 pt while reader in history | newest row +300 pt at newest edge |
|---|---|---|---|---|---|---|---|
| `flipped` | +30 (reveals older, correct direction) | 0.0 | 0.0, then +30 over 3 × 10 | all 20 | all 20 | 0.0 | 0.0 (pinned) |
| `lazyPosition` (current) | +30 | ~0.0 | row left the screen | all 20 | 20 … 33 … 97 … | 0.0 | not measured (clip scroll overridden) |
| `lazyBottom` (`.sizeChanges` bottom) | +30 | 844 | 724 | – | – | – | – |
| `vstackBottom` | +30 | 4530 | 4320 | all 20 | – | – | – |
| `lazyPlain` | +30 | 844 | 850 | – | – | – | – |

Limits of this probe (read before trusting it):

- It ran on **macOS 27.0**, not macOS 26. Rerun it with the owner's probe on macOS 26.
- Events go straight to `NSScrollView.scrollWheel(with:)` of the hosting scroll view, built from
  `CGEvent` with scroll phase fields. Real trackpad input goes through the window and responsive
  scrolling. The owner's probe is closer to real input.
- The synthetic **momentum** events did not scroll any mode (the offset did not change), so the
  momentum row is **inconclusive**. It must be checked with a real trackpad fling.
- `List` (mode `list`) gave frames that the probe cannot compare, so `List` is **not evaluated**.
- Row positions come from `onGeometryChange` + `frame(in: .global)`. A check in `flipped` mode
  printed the visible rows top to bottom as `[942, 943, 944, …]` (oldest at the top), and a window
  snapshot showed the same order, so the global frames include the flip transforms.

## Caveats of the inverted list, with sources

1. **Edges and anchors are mirrored.** `.top` in any scroll API means the visual bottom:
   `contentMargins`, `defaultScrollAnchor`, `ScrollViewReader.scrollTo(_, anchor:)`, `scrollPosition`
   anchors, and scroll-edge effects. Stream Chat hides the scroll edge effect on the logical top for
   this reason: "The flipped list's logical top edge is the composer edge, where iOS 27's scroll
   blur can snapshot media while the viewport resizes." (GetStream/stream-chat-swiftui
   `Sources/StreamChatSwiftUI/ChatMessageList/MessageListView.swift:340-342`, commit 19a2d7f5). On
   macOS 26 check `.scrollEdgeEffectStyle` / `.scrollEdgeEffectHidden` on both edges.
2. **Input direction and platform controls.** The DTS answer for watchOS says the flip inverts the
   Digital Crown: "You could either try reversing the underlying data but that won't invert the
   scroll direction." (https://developer.apple.com/forums/thread/769695, DTS Engineer, July 2025).
   On macOS the probe measured the **correct** wheel direction (+dy revealed older rows). Not
   verified: scroller knob drag, Page Up / Page Down / Home / End keys, and rubber-band feel. Test
   these by hand on macOS 26.
3. **Context menus, text selection, accessibility.** Forum reports show iOS 15-18 context menu
   bugs with a flipped container (https://developer.apple.com/forums/thread/765290,
   https://developer.apple.com/forums/thread/723770, and the OP of
   https://developer.apple.com/forums/thread/781282 says `.scaleEffect(y: -1)` "broke x position and
   `.contextMenu`"). None of these are Apple answers and none are macOS. Not verified on macOS:
   drag-select text across rows (the transcript uses `.textSelection(.enabled)`), context menu
   placement, and VoiceOver reading order. The view tree is newest first, so VoiceOver will likely
   read newest first (unverified). Test these three by hand before shipping.
4. **Apple has no first-party "inverted" API.** A 2021 forum request asks for
   `.scrollDirection(.inverted)` and lists flip side effects (nav bar, pull-to-refresh, scroll
   indicator direction) (https://developer.apple.com/forums/thread/681833, FB9148104, no Apple
   answer). The trick is widely used but not endorsed by Apple docs.

## Other options in detail

### `defaultScrollAnchor(.bottom, for: .sizeChanges)`

- Doc: the `.sizeChanges` role is "The role that influences how a scroll view should adjust its
  content offset when the scroll view's content or container size changes." macOS 15.0+
  (https://developer.apple.com/documentation/swiftui/scrollanchorrole/sizechanges).
  `defaultScrollAnchor(_:for:)` lists the roles and gives no statement about content inserted
  above a mid-scroll viewport
  (https://developer.apple.com/documentation/swiftui/view/defaultscrollanchor(_:for:)).
- `ScrollPosition` doc: "For an edge, that means keeping a top aligned scroll view scrolled to the
  top if the content size changes." (https://developer.apple.com/documentation/swiftui/scrollposition).
- Measured: when the reader is mid-content, a prepend kept the same top offset (`clipY` stayed
  370) and the row moved 844 pt (lazy) or 4530 pt (VStack). So the bottom anchor did not keep the
  reader's row. It is for "stay at the bottom edge", not for "keep this row". (Behavior when the
  reader sits exactly at the bottom edge was not measured.)
- No Apple engineer answer on the forums covers `.sizeChanges` with prepends (searched; two
  unanswered threads: https://developer.apple.com/forums/thread/781282,
  https://developer.apple.com/forums/thread/740490).

### `scrollPosition(id:anchor:)` (current code)

The doc promises to keep the view visible only for these events: "The data backing the content of
a scroll view is re-ordered", "The size of the scroll view changes", and the initial layout
(https://developer.apple.com/documentation/swiftui/view/scrollposition(id:anchor:)). Insert during
a live gesture is not in the list. This matches the owner's probe (−8 → 661 pt) and this probe
(row left the screen).

### Non-lazy `VStack` for bounded rows

WWDC26 321: "Unlike a VStack, a LazyVStack does not evaluate or render views that aren't
visible … But there is a correctness cost." A `VStack` has exact heights, so there is no estimate
jitter (probe: constant steps), but a prepend still moves the reader by the full inserted height.
It fixes nothing alone. Inside the inverted list, a `VStack` is an option if the row count is
bounded and late height changes remain a problem; the lazy version already measured constant
steps.

### AppKit non-flipped document view

`NSView.isFlipped` doc: "In a non-flipped coordinate system, the origin is in the lower-left corner
of the view and positive y-values extend upward." (https://developer.apple.com/documentation/appkit/nsview/isflipped).
With a non-flipped document view, the clip view origin is measured from the bottom, so growth at
the top does not move the visible rows (inference from the geometry; not measured here). Telegram
for macOS builds its chat history this way: `TableView(frame: …, isFlipped: false)`
(overtake/TelegramSwift `Telegram-Mac/ChatController.swift:575`, commit 579cebbf) with
`TGFlipableTableView.isFlipped` returning `flip`
(`packages/TGUIKit/Sources/TableView.swift:502, 554-556`). It still saves and restores scroll
state around some updates (`TableView.swift:3246-3252`). This is the native macOS version of the
same idea, but it means an `NSViewRepresentable` table with hosted SwiftUI cells, which is far more
code.

### Libraries

- Stream Chat SwiftUI uses the flip: `FlippedUpsideDown` =
  `.rotationEffect(.radians(Double.pi)).scaleEffect(x: -1, y: 1, anchor: .center)`
  (`Sources/StreamChatSwiftUI/ChatChannel/Utils/ChatChannelHelpers.swift:7-13`), applied to the
  `ScrollView` (`MessageListView.swift:397`) and to rows (`:229`, `:312`). Its package lists
  `.macOS(.v11)`, but it is an iOS chat SDK; macOS use is not verified.
- FluidGroup `swiftui-messaging-ui` (https://github.com/FluidGroup/swiftui-messaging-ui) says
  "contentOffset Adjustment is Fragile" and uses "a 100-million-point content space where items are
  anchored at the center". It is `UICollectionView` based, iOS 17+ only.

## Verification checklist for the owner (macOS 26, real trackpad)

1. Fling up hard so momentum is running when the older page lands. The reader's row must not move.
2. Scroll up slowly into rows with long markdown (over 4096 characters). Steps must be even.
3. Stream a reply while at the bottom (must follow) and while reading history (must not move).
4. Drag-select text across two rows, open a context menu, use Page Up/Down, Home/End, drag the
   scroller knob, and run VoiceOver over three rows.

<details>
<summary>Probe source (single file; build: <code>xcrun swiftc -swift-version 5 -O -o probe main.swift</code>; run: <code>./probe flipped</code>, <code>LATE=1 ./probe lazyPosition</code>)</summary>

```swift
import AppKit
import SwiftUI

// Probe: does a row stay put when older rows are prepended, idle and mid-gesture?
// Modes: lazyBottom (LazyVStack + defaultScrollAnchor(.bottom, for: .sizeChanges)),
//        vstackBottom (VStack + same), lazyPlain (LazyVStack, no anchor),
//        flipped (inverted LazyVStack, older rows appended at the end).

struct Row: Identifiable, Equatable { let id: Int; var h: CGFloat { 24 + CGFloat((id * 37) % 170) } }

final class Model: ObservableObject {
    @Published var rows: [Row] = []
    var frames: [Int: CGRect] = [:]
    @Published var top: Int?
    @Published var grow: Int = -1
    @Published var growBy: CGFloat = 0
}

let mode = CommandLine.arguments.dropFirst().first ?? "lazyBottom"
let flipped = mode == "flipped"
let model = Model()
var nextOld = 1000
func older(_ n: Int) -> [Row] { defer { nextOld -= n }; return (nextOld - n ..< nextOld).map { Row(id: $0) } }
// chronological order: oldest first
model.rows = older(60) + (1000..<1060).map { Row(id: $0 + 1000) }

let late = ProcessInfo.processInfo.environment["LATE"] != nil
struct RowView: View {
    let row: Row
    var g: CGFloat = 0
    @State private var extra: CGFloat = 0
    var body: some View {
        Text("row \(row.id)").frame(maxWidth: .infinity, minHeight: row.h + extra + g, alignment: .topLeading)
            // Like a markdown row whose parsed blocks arrive in a .task after it appears.
            .task { guard late else { return }; try? await Task.sleep(for: .milliseconds(30)); extra = CGFloat((row.id * 13) % 90) }
            .background(Color(hue: Double(row.id % 10) / 10, saturation: 0.3, brightness: 0.9))
            .onGeometryChange(for: CGRect.self) { $0.frame(in: .global) } action: { model.frames[row.id] = $0 }
            .onDisappear { model.frames[row.id] = nil }
    }
}

extension View {
    @ViewBuilder func flip(_ on: Bool) -> some View {
        if on { self.rotationEffect(.radians(.pi)).scaleEffect(x: -1, y: 1) } else { self }
    }
}

struct Probe: View {
    @ObservedObject var m: Model
    var body: some View {
        switch mode {
        case "flipped":
            ScrollView {
                LazyVStack(spacing: 4) { ForEach(m.rows.reversed()) { RowView(row: $0, g: $0.id == m.grow ? m.growBy : 0).flip(true) } }
            }
            .flip(true)
        case "vstackBottom":
            ScrollView { VStack(spacing: 4) { ForEach(m.rows) { RowView(row: $0, g: $0.id == m.grow ? m.growBy : 0) } } }
                .defaultScrollAnchor(.bottom, for: .sizeChanges)
        case "lazyPosition":
            ScrollView { LazyVStack(spacing: 4) { ForEach(m.rows) { RowView(row: $0, g: $0.id == m.grow ? m.growBy : 0) } }.scrollTargetLayout() }
                .scrollPosition(id: $m.top, anchor: .top)
        case "list":
            List(m.rows) { RowView(row: $0, g: $0.id == m.grow ? m.growBy : 0) }
        case "lazyPlain":
            ScrollView { LazyVStack(spacing: 4) { ForEach(m.rows) { RowView(row: $0, g: $0.id == m.grow ? m.growBy : 0) } } }
        default:
            ScrollView { LazyVStack(spacing: 4) { ForEach(m.rows) { RowView(row: $0, g: $0.id == m.grow ? m.growBy : 0) } } }
                .defaultScrollAnchor(.bottom, for: .sizeChanges)
        }
    }
}

func findScrollView(_ v: NSView) -> NSScrollView? {
    if let s = v as? NSScrollView { return s }
    for sub in v.subviews { if let s = findScrollView(sub) { return s } }
    return nil
}

let app = NSApplication.shared
app.setActivationPolicy(.accessory)
let window = NSWindow(contentRect: NSRect(x: 100, y: 100, width: 500, height: 600),
                      styleMask: [.titled], backing: .buffered, defer: false)
window.contentView = NSHostingView(rootView: Probe(m: model))
window.orderFrontRegardless()

func wheel(_ dy: Int32, phase: Int64 = 0, momentum: Int64 = 0) {
    guard let sv = findScrollView(window.contentView!),
          let cg = CGEvent(scrollWheelEvent2Source: nil, units: .pixel, wheelCount: 1, wheel1: dy, wheel2: 0, wheel3: 0)
    else { return }
    cg.setIntegerValueField(.scrollWheelEventIsContinuous, value: 1)
    cg.setIntegerValueField(.scrollWheelEventScrollPhase, value: phase)
    cg.setIntegerValueField(.scrollWheelEventMomentumPhase, value: momentum)
    let f = window.frame
    cg.location = CGPoint(x: f.midX, y: (NSScreen.screens[0].frame.height - f.midY))
    if let ev = NSEvent(cgEvent: cg) { sv.scrollWheel(with: ev) }
}

func pump(_ s: Double) async { try? await Task.sleep(for: .seconds(s)) }
func y(_ id: Int) -> CGFloat { model.frames[id]?.minY ?? .nan }
/// A row near the visual top of the viewport.
func refRow() -> Int {
    let visible = model.frames.filter { $0.value.minY > 40 && $0.value.minY < 300 }
    return visible.min { $0.value.minY < $1.value.minY }!.key
}
func prepend(_ n: Int) { model.rows.insert(contentsOf: older(n), at: 0) }
func p(_ s: String) {
    let sv = findScrollView(window.contentView!)!
    print("[\(mode)] \(s)  | clipY=\(sv.contentView.bounds.origin.y) docH=\(sv.documentView!.frame.height)")
}

Task { @MainActor in
    await pump(1.0)
    // Move near the oldest rows: jump the clip view once while idle (setup only).
    let sv = findScrollView(window.contentView!)!
    let docH = sv.documentView!.frame.height
    let target: CGFloat = flipped ? docH - sv.contentView.bounds.height - 400 : 400
    sv.contentView.scroll(to: NSPoint(x: 0, y: target)); sv.reflectScrolledClipView(sv.contentView)
    await pump(0.5)

    // 1. Wheel direction: +dy should reveal older rows (content moves down on screen).
    var r = refRow(); var before = y(r)
    wheel(0, phase: 1); await pump(0.05)
    for _ in 0..<3 { wheel(10, phase: 2); await pump(0.05) }
    wheel(0, phase: 4); await pump(0.3)
    let revealsOlder = y(r) - before > 0
    p("wheel +30 moved row \(r) by \(y(r) - before) pt (positive = content moved down = reveals older)")

    // 2. Idle prepend.
    await pump(0.5)
    r = refRow(); before = y(r)
    prepend(40); await pump(0.5)
    p("idle prepend: row \(r) moved \(y(r) - before) pt")

    // 3. Mid-gesture prepend (tracking phase).
    let dir: Int32 = revealsOlder ? 10 : -10 // move toward older rows whatever the wheel sign does
    wheel(0, phase: 1)
    for _ in 0..<3 { wheel(dir, phase: 2); await pump(0.02) }
    r = refRow(); before = y(r)
    prepend(40); await pump(0.1)
    let afterInsert = y(r)
    for _ in 0..<3 { wheel(dir, phase: 2); await pump(0.02) }
    wheel(0, phase: 4); await pump(0.2)
    p("mid-gesture prepend: row \(r) moved \(afterInsert - before) pt at insert; \(y(r) - afterInsert) pt over the next 3 events")

    // 4. Momentum prepend.
    wheel(0, phase: 1); wheel(dir, phase: 2); wheel(0, phase: 4)
    wheel(dir, momentum: 1)
    for _ in 0..<3 { wheel(dir, momentum: 2); await pump(0.02) }
    r = refRow(); before = y(r)
    prepend(40); await pump(0.1)
    let afterMomentum = y(r)
    for _ in 0..<3 { wheel(dir, momentum: 2); await pump(0.02) }
    wheel(0, momentum: 3); await pump(0.2)
    p("momentum prepend: row \(r) moved \(afterMomentum - before) pt at insert; \(y(r) - afterMomentum) pt over the next 3 events")

    // 5. Scroll up through the new estimated rows: record the ref row's per-event step.
    r = refRow(); var last = y(r); var steps: [Int] = []
    wheel(0, phase: 1)
    for _ in 0..<40 { wheel(dir * 2, phase: 2); await pump(0.02); let n = y(r); if n.isNaN { break }; steps.append(Int((n - last).rounded())); last = n }
    wheel(0, phase: 4)
    p("per-event steps while scrolling into new rows (want constant): \(steps)")
    // 6. Newest row grows (streaming) while the reader is in history.
    await pump(0.5)
    r = refRow(); before = y(r)
    model.grow = model.rows.last!.id; model.growBy = 300; await pump(0.3)
    p("newest row grew 300 pt off screen: row \(r) moved \(y(r) - before) pt")
    // 7. Reader at the newest edge while the newest row grows: does it stay pinned?
    let sv2 = findScrollView(window.contentView!)!
    let edge: CGFloat = flipped ? 0 : sv2.documentView!.frame.height - sv2.contentView.bounds.height
    sv2.contentView.scroll(to: NSPoint(x: 0, y: edge)); sv2.reflectScrolledClipView(sv2.contentView)
    await pump(0.5)
    let newest = model.rows.last!.id
    let b = model.frames[newest]?.maxY ?? .nan; p("before: h \(model.frames[newest]?.height ?? .nan) minY \(model.frames[newest]?.minY ?? .nan)")
    model.growBy = 600; await pump(0.3)
    p("at newest edge, newest row grew 300 pt: its bottom moved \((model.frames[newest]?.maxY ?? .nan) - b) pt (0 = pinned); height \(model.frames[newest]?.height ?? .nan), minY \(model.frames[newest]?.minY ?? .nan)")
    exit(0)
}
app.run()
```

</details>
