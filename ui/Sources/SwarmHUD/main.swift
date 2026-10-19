import AppKit
import SwarmHUDCore
import SwiftUI

// `SwarmHUD --snapshot <dir> --fixture <usage.json> [--closed <usage.json>] [--history <history.json>]
// [--working <working.json>] [--now <seconds>]` renders the HUD to PNG files with no window, for
// design review and tests: open and closed for every phase, the closed wings for each mascot family
// and state, the ring's edge cases, a sheet of every own mascot frame, and a sheet of every guest
// mascot in each state at the closed and the open size. `--closed` is the reading
// for the closed renders (default: `--fixture`). Without `--snapshot` the HUD runs.
let arguments = CommandLine.arguments
if let at = arguments.firstIndex(of: "--snapshot"), arguments.count > at + 1 {
    MainActor.assumeIsolated { snapshot(to: URL(fileURLWithPath: arguments[at + 1]), arguments: arguments) }
    exit(0)
}

let delegate = MainActor.assumeIsolated { AppDelegate() }
let app = NSApplication.shared
app.delegate = delegate
app.run()

@MainActor
func snapshot(to directory: URL, arguments: [String]) {
    func value(_ flag: String) -> String? {
        arguments.firstIndex(of: flag).flatMap { arguments.indices.contains($0 + 1) ? arguments[$0 + 1] : nil }
    }
    guard let fixture = value("--fixture"),
          let data = FileManager.default.contents(atPath: fixture),
          let usage = try? UsageSnapshot.decode(data) else {
        FileHandle.standardError.write(Data("SwarmHUD: --snapshot needs --fixture <swarm usage json>\n".utf8))
        exit(2)
    }
    let now = Date(timeIntervalSince1970: value("--now").flatMap(Double.init) ?? Date().timeIntervalSince1970)
    func fail(_ text: String) -> Never {
        FileHandle.standardError.write(Data("SwarmHUD: \(text)\n".utf8))
        exit(1)
    }
    do {
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    } catch {
        fail("cannot create \(directory.path): \(error.localizedDescription)")
    }
    let model = UsageModel()
    func readFile(_ flag: String) -> Data? { value(flag).flatMap { FileManager.default.contents(atPath: $0) } }
    let openAccounts = accounts(from: usage.meters)
    let closedMeters = readFile("--closed").flatMap { try? UsageSnapshot.decode($0) }?.meters ?? usage.meters
    let closedAccounts = accounts(from: closedMeters)
    // Fixture history only: the owner's saved history would make the pictures change from run to run.
    model.history = readFile("--history").flatMap { try? JSONDecoder().decode([HistorySample].self, from: $0) } ?? []
    let fileAgents = readFile("--working").map(WorkingSnapshot.read) ?? WorkingSnapshot()
    model.readAt = now.addingTimeInterval(-120)
    model.now = now
    // Swarm's own art, so a render does not depend on whose turn the fixed clock picks.
    model.fixedMascot = 0

    func write(_ view: some View, _ name: String) {
        let renderer = ImageRenderer(content: view)
        renderer.scale = 2
        let url = directory.appending(path: "\(name).png")
        guard let image = renderer.cgImage,
              let png = NSBitmapImageRep(cgImage: image).representation(using: .png, properties: [:]) else {
            fail("cannot render \(url.lastPathComponent)")
        }
        do { try png.write(to: url) } catch { fail("cannot write \(url.path): \(error.localizedDescription)") }
        print(url.path)
    }
    func render(_ name: String, open: Bool) {
        model.isOpen = open
        write(HUDView(model: model, live: false)
            .frame(width: Layout.openWidth + 40, height: open ? model.openSize.height + 40 : 80, alignment: .top)
            .background(Color(hex: 0x6F8FB8)), name)
    }

    for phase in DayPhase.allCases {
        // Set the phase itself: `pinnedPhase` would overwrite the owner's saved theme pin.
        model.phase = phase
        model.agents = fileAgents
        model.lastActive = now
        model.accounts = closedAccounts
        render("\(phase.rawValue)-closed", open: false)
        model.accounts = openAccounts
        render("\(phase.rawValue)-open", open: true)
    }

    model.phase = .day
    model.accounts = closedAccounts
    for family in ["claude", "codex"] {
        for state in MascotState.allCases {
            model.lastWorking = family
            model.agents = switch state {
            case .working: WorkingSnapshot(working: [family: 2])
            case .waiting: WorkingSnapshot(waiting: [family: 1])
            case .idle, .sleeping: WorkingSnapshot()
            }
            model.lastActive = now.addingTimeInterval(state == .sleeping ? -301 : -60)
            render("closed-\(family)-\(state.rawValue)", open: false)
        }
    }

    // The ring's edge cases, beside an idle Claude mascot.
    model.lastWorking = "claude"
    model.agents = WorkingSnapshot()
    model.lastActive = now
    let at = { (pct: Int) in
        accounts(from: closedMeters.map {
            UsageMeter(provider: $0.provider, account: $0.account, label: $0.label, window: $0.window,
                       windowMinutes: $0.windowMinutes, usedPct: $0.usedPct.map { _ in pct },
                       resetTimeSeconds: $0.resetTimeSeconds, state: $0.state, asOfSeconds: $0.asOfSeconds)
        })
    }
    for (name, list, readError) in [("warn", at(88), nil), ("full", at(100), nil), ("stale", closedAccounts, "timed out"),
                                    ("none", [], nil)] as [(String, [UsageAccount], String?)] {
        model.accounts = list
        model.readError = readError
        render("closed-ring-\(name)", open: false)
    }
    model.readError = nil

    // Every own frame at the closed size, a row per family, for judging the art.
    let ticks: [MascotState: [Int]] = [.working: [0, 1], .waiting: [0, 1], .idle: [0, 13], .sleeping: [0, 1, 2]]
    write(VStack(spacing: 4) {
        ForEach(mascotFamilies + ["bee"], id: \.self) { family in
            HStack(spacing: 4) {
                ForEach(MascotState.allCases, id: \.self) { state in
                    ForEach(ticks[state]!, id: \.self) { tick in
                        MascotCanvas(family: family, state: state, turn: 0, height: Layout.closedMascot,
                                     tick: tick, seconds: Double(tick), crowd: tick + 2)
                            .frame(width: Layout.wing - Layout.closedEar, height: Layout.minNotch)
                            .background(Color.black)
                    }
                }
            }
        }
    }
    .padding(4)
    .background(Color(hex: 0x6F8FB8)), "mascot-frames")

    // Every turn in each state, still, at the closed size and then the open size.
    write(VStack(alignment: .leading, spacing: 4) {
        ForEach(0..<MascotCast.count, id: \.self) { turn in
            HStack(spacing: 4) {
                Text(MascotCast.name(turn)).font(.system(size: 11)).foregroundStyle(.white)
                    .frame(width: 90, alignment: .leading)
                ForEach(MascotState.allCases, id: \.self) { state in
                    MascotCanvas(family: "claude", state: state, turn: turn, height: Layout.closedMascot, tick: nil, seconds: nil)
                        .frame(width: Layout.wing - Layout.closedEar, height: Layout.minNotch)
                        .background(Color.black)
                }
                ForEach(MascotState.allCases, id: \.self) { state in
                    MascotCanvas(family: "claude", state: state, turn: turn, height: Layout.openMascot, tick: nil, seconds: nil)
                        .frame(width: 2 * Layout.openMascot, height: Layout.openMascot + 12)
                        .background(Color.black)
                }
            }
        }
    }
    .padding(4)
    .background(Color(hex: 0x6F8FB8)), "mascot-cast")
}
