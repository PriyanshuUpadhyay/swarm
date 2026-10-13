# One queued alert per window: when SwiftUI drops an alert on macOS

Date: 2026-10-08. Scope: macOS 27 SwiftUI app, Swift 6, the window alerts in
`ui/Sources/Swarm/SwarmApp.swift` (`WindowAlert`). A scratch app, run unbundled with a clean
`HOME`, printed the window's attached sheet and its text after each step.

## Result

Use **one `.alert` per window**. It presents a shown item, a new alert waits in a list, and the
next item shows one `Task` hop after an `onChange` sees the shown item become nil.

Example (measured): alert A is up, and the code asks for a different `.alert` B. Nothing shows,
and B's state stays `true`, so every later notice that waits for B to close waits forever. With
one queued alert, three items asked for in a row each show in turn with their own text.

Rule: never ask for an alert while another one is up or closing. Ask the queue, and let it show
the next one after the close.

## Measurements

| Step | Result |
|---|---|
| Alert asked for while a sheet is up | Waits, then shows after the sheet closes |
| Sheet asked for while an alert is up | Waits, then shows after the alert closes |
| Different `.alert` asked for while one is up | Dropped; its state stays `true` |
| Different `.alert` asked for in the tick, or the `onChange`, of the other's close | Dropped |
| Different `.alert` asked for one hop later (`Task {}`, `DispatchQueue.main.async`, 100 ms, 400 ms) | Shows |
| Same `.alert` shown again from the `onChange` of its own close | Shows |
| One `.alert` over a list whose first item changes in the tick it closes | Panel keeps the old text |
| One `.alert` over a shown item plus a list, hop from `onChange` | Each item shows in turn |
| Append to the list and close the shown item in one step, hop from `onChange` | Next item shows with its own text |

Limit: alerts were closed by code, not by a button press, because the probe used no synthetic
input.

## Probes

Two separate `.alert` modifiers and a sheet (modes `sheet`, `alert`, `chain`, `sheetchain`,
`onchange`, `reshow`, `chain100` and the like):

```swift
import AppKit
import SwiftUI

// Case from argv: "sheet" opens a sheet, then asks for an alert while it is up, then closes the sheet.
// "alert" opens confirm alert A, asks for alert B while A is up, then closes A.
let mode = CommandLine.arguments.dropFirst().first ?? "sheet"

func report(_ label: String) {
    let window = NSApp.windows.first { $0.isVisible && $0.attachedSheet != nil || $0.isVisible && $0.className.contains("AppKitWindow") }
    let sheet = NSApp.windows.compactMap(\.attachedSheet).first
    let texts = sheet.map { s in
        (s.contentView.map { all($0) } ?? []).compactMap { ($0 as? NSTextField)?.stringValue }.filter { !$0.isEmpty }
    } ?? []
    print("\(label): window=\(window != nil) sheet=\(sheet.map { String(describing: type(of: $0)) } ?? "none") texts=\(texts)")
    fflush(stdout)
}
func all(_ v: NSView) -> [NSView] { [v] + v.subviews.flatMap(all) }

struct Probe: View {
    @State var showSheet = false
    @State var alertA = false
    @State var alertB = false
    @State var reshown = false
    var body: some View {
        Text("probe").frame(width: 300, height: 200)
            .sheet(isPresented: $showSheet) { Text("SHEET").frame(width: 200, height: 100) }
            .alert("ALERT-A", isPresented: $alertA) { Button("OK") {} }
            .alert("ALERT-B", isPresented: $alertB) { Button("OK") {} }
            .onChange(of: alertA) { _, up in
                if !up, mode == "onchange" { alertB = true }
                if !up, mode == "chainasync" { DispatchQueue.main.async { alertB = true } }
                if !up, mode == "chainyield" { Task { alertB = true } }
                if !up, mode == "chain100" { Task { try? await Task.sleep(for: .milliseconds(100)); alertB = true } }
                if !up, mode == "chain400" { Task { try? await Task.sleep(for: .milliseconds(400)); alertB = true } }
                if !up, mode == "reshow", !reshown { reshown = true; alertA = true }
                if !up, mode == "reshowlater", !reshown { reshown = true; Task { try? await Task.sleep(for: .milliseconds(400)); alertA = true } }
            }
            .task {
                try? await Task.sleep(for: .seconds(1))
                if mode.hasPrefix("sheet") { showSheet = true } else { alertA = true }
                try? await Task.sleep(for: .seconds(1))
                report("first up")
                if mode.hasPrefix("reshow") { alertA = false; try? await Task.sleep(for: .seconds(2)); report("after reshow"); print("alertA state=\(alertA)"); exit(0) }
                if mode == "onchange" || mode.hasPrefix("chain") && mode != "chain" { alertA = false; try? await Task.sleep(for: .seconds(2)); report("after onChange"); print("alertB state=\(alertB)"); exit(0) }
                if mode == "chain" { alertA = false; alertB = true; try? await Task.sleep(for: .seconds(2)); report("same tick"); print("alertB state=\(alertB)"); exit(0) }
                if mode == "sheetchain" { showSheet = false; alertB = true; try? await Task.sleep(for: .seconds(2)); report("same tick"); print("alertB state=\(alertB)"); exit(0) }
                alertB = true
                try? await Task.sleep(for: .seconds(1))
                report("B asked")
                if mode == "sheet" { showSheet = false } else { alertA = false }
                try? await Task.sleep(for: .seconds(2))
                report("first closed")
                print("alertB state=\(alertB)")
                exit(0)
            }
    }
}

struct ProbeApp: App {
    var body: some Scene { WindowGroup { Probe() } }
}
ProbeApp.main()
```

One queued alert (modes `queue`, `alertsheet`, `hop`, `button`):

```swift
import AppKit
import SwiftUI

// queue: one .alert(presenting:) over a queue; closing the first leaves the second as first in the same tick.
// alertsheet: a sheet asked for while an alert is up.
let mode = CommandLine.arguments.dropFirst().first ?? "queue"
func all(_ v: NSView) -> [NSView] { [v] + v.subviews.flatMap(all) }
func report(_ label: String) {
    let sheet = NSApp.windows.compactMap(\.attachedSheet).first
    let texts = sheet.map { s in (s.contentView.map { all($0) } ?? []).compactMap { ($0 as? NSTextField)?.stringValue }.filter { !$0.isEmpty } } ?? []
    print("\(label): sheet=\(sheet.map { String(describing: type(of: $0)) } ?? "none") texts=\(texts)"); fflush(stdout)
}
struct Item: Identifiable, Equatable { let id: String }
struct Probe: View {
    @State var queue: [Item] = []
    @State var showSheet = false
    @State var shown: Item?
    func enqueue(_ item: Item) { queue.append(item); Task { next() } }
    func next() { guard shown == nil, !queue.isEmpty else { return }; shown = queue.removeFirst() }
    var body: some View {
        Text("probe").frame(width: 300, height: 200)
            .sheet(isPresented: $showSheet) { Text("SHEET").frame(width: 200, height: 100) }
            .alert(mode == "hop" || mode == "button" ? shown?.id ?? "" : queue.first?.id ?? "", isPresented: mode == "hop" || mode == "button"
                    ? Binding(get: { shown != nil }, set: { if !$0 { shown = nil } })
                    : Binding(get: { !queue.isEmpty }, set: { if !$0, !queue.isEmpty { queue.removeFirst() } }),
                   presenting: mode == "hop" || mode == "button" ? shown : queue.first) { _ in Button("OK") {} }
            .onChange(of: shown == nil) { _, empty in if empty { Task { next() } } }
            .onChange(of: queue.count) { _, _ in if mode == "button" { Task { next() } } }
            .task {
                try? await Task.sleep(for: .seconds(1))
                queue = [Item(id: "Q-ONE"), Item(id: "Q-TWO")]
                try? await Task.sleep(for: .seconds(1))
                report("first")
                if mode == "button" {
                    queue = []
                    queue.append(Item(id: "B-ONE"))
                    try? await Task.sleep(for: .seconds(1))
                    report("button first")
                    queue.append(Item(id: "B-TWO")); shown = nil
                    try? await Task.sleep(for: .seconds(2))
                    report("button after close"); print("queue=\(queue.map(\.id))"); exit(0)
                }
                if mode == "hop" {
                    queue = []
                    enqueue(Item(id: "H-ONE")); enqueue(Item(id: "H-TWO"))
                    try? await Task.sleep(for: .seconds(1))
                    report("hop first")
                    shown = nil
                    try? await Task.sleep(for: .seconds(2))
                    report("hop after close")
                    enqueue(Item(id: "H-THREE"))
                    shown = nil
                    try? await Task.sleep(for: .seconds(2))
                    report("hop third"); print("queue=\(queue.map(\.id))"); exit(0)
                }
                if mode == "alertsheet" {
                    showSheet = true
                    try? await Task.sleep(for: .seconds(1))
                    report("sheet asked")
                    queue = []
                    try? await Task.sleep(for: .seconds(2))
                    report("alert closed"); print("showSheet=\(showSheet)"); exit(0)
                }
                // Close the shown alert the way its button does: through the binding's set.
                queue.removeFirst()
                try? await Task.sleep(for: .seconds(2))
                report("after close"); print("queue=\(queue.map(\.id))"); exit(0)
            }
    }
}
struct ProbeApp: App { var body: some Scene { WindowGroup { Probe() } } }
ProbeApp.main()
```
