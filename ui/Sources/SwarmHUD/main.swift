import AppKit
import SwarmHUDCore
import SwiftUI

// `SwarmHUD --snapshot <dir> --fixture <usage.json> [--now <seconds>]` renders the closed and open
// HUD for every phase to PNG files, for design review and tests. Without it the HUD runs.
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
    model.accounts = accounts(from: usage.meters)
    model.readAt = now.addingTimeInterval(-120)
    model.now = now
    for phase in DayPhase.allCases {
        // Set the phase itself: `pinnedPhase` would overwrite the owner's saved theme pin.
        model.phase = phase
        for (name, open, fold) in [("closed", false, false), ("open", true, false), ("open-fold", true, true)] {
            model.isOpen = open
            model.foldOpen = fold
            let view = HUDView(model: model, live: false)
                .frame(width: 720, height: open ? model.openSize.height + 40 : 80, alignment: .top)
                .background(Color(hex: 0x6F8FB8))
            let renderer = ImageRenderer(content: view)
            renderer.scale = 2
            let url = directory.appending(path: "\(phase.rawValue)-\(name).png")
            guard let image = renderer.cgImage,
                  let png = NSBitmapImageRep(cgImage: image).representation(using: .png, properties: [:]) else {
                fail("cannot render \(url.lastPathComponent)")
            }
            do { try png.write(to: url) } catch { fail("cannot write \(url.path): \(error.localizedDescription)") }
            print(url.path)
        }
    }
}
