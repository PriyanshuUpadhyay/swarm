import AppKit
import SwarmHUDCore
import SwiftUI

// `SwarmHUD --snapshot <dir> --fixture <usage.json> [--closed <usage.json>] [--history <history.json>]
// [--working <working.json>] [--now <seconds>]` renders the HUD to PNG files with no window, for
// design review and tests: open and closed for every phase, the closed wings for each mascot family
// and state, the ring's edge cases, a sheet of every own mascot frame, and a sheet of every guest
// mascot in each state at the closed and the open size. `--closed` is the reading
// for the closed renders (default: `--fixture`). `SwarmHUD --app-icon <dir.iconset>` writes
// Swarm.app's icon for `iconutil`. Without either flag the HUD runs.
let arguments = CommandLine.arguments
if let at = arguments.firstIndex(of: "--snapshot"), arguments.count > at + 1 {
    // The owner's own Codex pets would make the cast sheet differ by machine; set CODEX_HOME to
    // render a test pet.
    if ProcessInfo.processInfo.environment["CODEX_HOME"] == nil { setenv("CODEX_HOME", "/var/empty", 1) }
    MainActor.assumeIsolated { snapshot(to: URL(fileURLWithPath: arguments[at + 1]), arguments: arguments) }
    exit(0)
}
if let at = arguments.firstIndex(of: "--app-icon"), arguments.count > at + 1 {
    appIcon(to: URL(fileURLWithPath: arguments[at + 1]))
    exit(0)
}

let delegate = MainActor.assumeIsolated { AppDelegate() }
let app = NSApplication.shared
app.delegate = delegate
app.run()

/// The Deno guest's still idle frame on the page's `#101010`, in a rounded square, at each iconset
/// size. The pixels scale with no smoothing, so the art stays sharp.
func appIcon(to directory: URL) {
    guard let deno = SheetMascot.all.first(where: { $0.name == "Deno" }).flatMap(Guest.init),
          let art = deno.frame(.idle, tick: nil) else {
        FileHandle.standardError.write(Data("SwarmHUD: no Deno sheet\n".utf8))
        exit(1)
    }
    try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    for (points, scale) in [(16, 1), (16, 2), (32, 1), (32, 2), (128, 1), (128, 2), (256, 1), (256, 2), (512, 1), (512, 2)] {
        let side = CGFloat(points * scale)
        guard let context = CGContext(data: nil, width: Int(side), height: Int(side), bitsPerComponent: 8, bytesPerRow: 0,
                                      space: CGColorSpace(name: CGColorSpace.sRGB)!,
                                      bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue) else { exit(1) }
        // Apple's grid: the square is 824 of 1024 with a 185 radius.
        let inset = side * 100 / 1024, square = side - 2 * inset
        context.addPath(CGPath(roundedRect: CGRect(x: inset, y: inset, width: square, height: square),
                               cornerWidth: square * 185 / 824, cornerHeight: square * 185 / 824, transform: nil))
        context.setFillColor(CGColor(srgbRed: 0x10 / 255, green: 0x10 / 255, blue: 0x10 / 255, alpha: 1))
        context.fillPath()
        let fit = square * 0.7 / max(deno.cell.width, deno.cell.height)
        let w = deno.cell.width * fit, h = deno.cell.height * fit
        context.interpolationQuality = .none
        context.draw(art, in: CGRect(x: (side - w) / 2, y: (side - h) / 2, width: w, height: h))
        let name = scale == 1 ? "icon_\(points)x\(points).png" : "icon_\(points)x\(points)@2x.png"
        guard let image = context.makeImage(),
              let png = NSBitmapImageRep(cgImage: image).representation(using: .png, properties: [:]),
              (try? png.write(to: directory.appending(path: name))) != nil else {
            FileHandle.standardError.write(Data("SwarmHUD: cannot write \(name)\n".utf8))
            exit(1)
        }
    }
}

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
