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
    try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    let model = UsageModel()
    model.accounts = accounts(from: usage.meters)
    model.readAt = now.addingTimeInterval(-120)
    model.now = now
    for phase in DayPhase.allCases {
        model.pinnedPhase = phase
        for (name, open, fold) in [("closed", false, false), ("open", true, false), ("open-fold", true, true)] {
            model.isOpen = open
            model.foldOpen = fold
            let view = HUDView(model: model, live: false)
                .frame(width: 720, height: open ? model.openSize.height + 40 : 80, alignment: .top)
                .background(Color(hex: 0x6F8FB8))
            let renderer = ImageRenderer(content: view)
            renderer.scale = 2
            guard let image = renderer.cgImage else { continue }
            let url = directory.appending(path: "\(phase.rawValue)-\(name).png")
            let rep = NSBitmapImageRep(cgImage: image)
            try? rep.representation(using: .png, properties: [:])?.write(to: url)
            print(url.path)
        }
    }
    model.pinnedPhase = nil
}
