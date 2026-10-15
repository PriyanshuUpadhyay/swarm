import ImageIO
import SwarmHUDCore
import SwiftUI

// The HUD's pixel mascot, in the closed wing and the open card. The idea of a notch pet that works,
// waits and sleeps with its agents comes from Notchi; that project is GPL-3.0, so this art and code
// are our own. Our own frames are grids of characters, `.` empty, one colour per character (the cue
// glyphs use `K`), 12 cells wide and 10 tall, drawn at whole device pixels. Open-licence guests
// take turns with it (`MascotSheets.swift`, `ui/MASCOT-NOTICE`).

typealias Sprite = [String]

struct MascotArt {
    let palette: [Character: Color]
    let idle: Sprite
    let blink: Sprite
    /// Two to four frames at 4 fps.
    let working: [Sprite]
    let waiting: Sprite
    let sleeping: Sprite

    static let columns = 12, rows = 10

    static func of(_ family: String) -> MascotArt { family == "codex" ? codex : claude }

    /// A chunky terracotta creature with ear nubs and big shiny eyes, typing at a small gray
    /// keyboard while it works. B body, H highlight, S shade, P cheek, E eye, W eye shine, K keys.
    static let claude = MascotArt(
        palette: [
            "B": Color(hex: 0xC8775A), "H": Color(hex: 0xE09A7D), "S": Color(hex: 0x9C5741),
            "P": Color(hex: 0xE58A80), "E": Color(hex: 0x24150F), "W": Color(hex: 0xFFF3EC),
            "K": Color(hex: 0x7D838D), "k": Color(hex: 0xB9BEC6),
        ],
        idle: [
            "............",
            "..BB....BB..",
            ".BBBBBBBBBB.",
            ".BHBBBBBBBB.",
            ".BWEBBBBWEB.",
            ".BEEBBBBEEB.",
            "BBPBBBBBBPBB",
            "BBBBBBBBBBBB",
            ".SBBBBBBBBS.",
            "..SS....SS..",
        ],
        blink: [
            "............",
            "..BB....BB..",
            ".BBBBBBBBBB.",
            ".BHBBBBBBBB.",
            ".BBBBBBBBBB.",
            ".BEEBBBBEEB.",
            "BBPBBBBBBPBB",
            "BBBBBBBBBBBB",
            ".SBBBBBBBBS.",
            "..SS....SS..",
        ],
        working: [
            [
                "............",
                "..BB....BB..",
                ".BBBBBBBBBB.",
                ".BHBBBBBBBB.",
                ".BBBBBBBBBB.",
                ".BWEBBBBWEB.",
                ".BEEBBBBEEB.",
                ".BPBBBBBBBBB",
                "kkHHkkkkkkkk",
                ".KKKKKKKKKK.",
            ],
            [
                "............",
                "..BB....BB..",
                ".BBBBBBBBBB.",
                ".BHBBBBBBBB.",
                ".BBBBBBBBBB.",
                ".BWEBBBBWEB.",
                ".BEEBBBBEEB.",
                "BBBBBBBBBPB.",
                "kkkkkkkkHHkk",
                ".KKKKKKKKKK.",
            ],
        ],
        waiting: [
            "............",
            "..BB....BB.B",
            ".BBBBBBBBBBB",
            ".BWEBBBBWEB.",
            ".BEEBBBBEEB.",
            ".BBBBBBBBBB.",
            "BBPBBBBBBPB.",
            "BBBBBBBBBBB.",
            ".SBBBBBBBBS.",
            "..SS....SS..",
        ],
        sleeping: [
            "............",
            "............",
            "..BB....BB..",
            ".BBBBBBBBBB.",
            ".BHBBBBBBBB.",
            ".BBBBBBBBBB.",
            ".BEEBBBBEEB.",
            "BBPBBBBBBPBB",
            "BBBBBBBBBBBB",
            ".SSSSSSSSSS.",
        ]
    )

    /// A little terminal box with a `>_` face. W frame, G shade, D screen, L prompt.
    static let codex = MascotArt(
        palette: [
            "W": Color(hex: 0xE4E6EA), "G": Color(hex: 0xA9AEB6), "D": Color(hex: 0x2A2E35),
            "L": Color(hex: 0xF4F6F8),
        ],
        idle: [
            "............",
            "..WWWWWWWW..",
            ".WWWWWWWWWW.",
            ".WDDDDDDDDW.",
            ".WDLDDDDDDW.",
            ".WDDLDDDDDW.",
            "WWDLDDLLDDWW",
            ".WDDDDDDDDW.",
            ".GWWWWWWWWG.",
            "..GG....GG..",
        ],
        blink: [
            "............",
            "..WWWWWWWW..",
            ".WWWWWWWWWW.",
            ".WDDDDDDDDW.",
            ".WDDDDDDDDW.",
            ".WDLLDDDDDW.",
            "WWDDDDLLDDWW",
            ".WDDDDDDDDW.",
            ".GWWWWWWWWG.",
            "..GG....GG..",
        ],
        working: [
            [
                "............",
                "..WWWWWWWW..",
                ".WWWWWWWWWW.",
                "WWDDDDDDDDW.",
                "WWDLDDDDDDW.",
                ".WDDLDDDDDW.",
                ".WDLDDLLDDWW",
                ".WDDDDDDDDWW",
                ".GWWWWWWWWG.",
                "..GG....GG..",
            ],
            [
                "............",
                "..WWWWWWWW..",
                ".WWWWWWWWWW.",
                ".WDDDDDDDDWW",
                ".WDLDDDDDDWW",
                ".WDDLDDDDDW.",
                "WWDLDDLDLLW.",
                "WWDDDDDDDDW.",
                ".GWWWWWWWWG.",
                "..GG....GG..",
            ],
        ],
        waiting: [
            "............",
            "..WWWWWWWW..",
            ".WWWWWWWWWW.",
            ".WDDDDDDDDW.",
            ".WDLDDDDDDW.",
            ".WDDLDLLDDW.",
            "WWDLDDLLDDWW",
            ".WDDDDDDDDW.",
            ".GWWWWWWWWG.",
            "..GG....GG..",
        ],
        sleeping: [
            "............",
            "............",
            "..WWWWWWWW..",
            ".WWWWWWWWWW.",
            ".WDDDDDDDDW.",
            ".WDDDDDDDDW.",
            "WWDLLDDDDDWW",
            ".WDDDDDDDDW.",
            ".GWWWWWWWWG.",
            ".GGGGGGGGGG.",
        ]
    )

    static let bang: Sprite = ["K", "K", "K", ".", "K"]
    static let zee: Sprite = ["KKKKK", "...K.", "..K..", ".K...", "KKKKK"]
}

extension MascotState {
    /// The own art's frame rate; a guest uses its sheet's rate, at most 8 fps.
    var fps: Double {
        switch self {
        case .working, .idle: return 4
        case .waiting: return 2
        case .sleeping: return 1
        }
    }
}

/// A guest mascot: a PNG sheet with a row per state and a column per frame (`MascotSheets.swift`).
struct SheetMascot {
    let id: String
    let name: String
    let credit: String
    /// The art draws its own sleep, so the HUD adds no `z`.
    let ownZ: Bool
    /// One frame, in sheet pixels.
    let cell: CGSize
    /// Frames and frame rate per row, in `rows` order.
    let frames: [Int]
    let fps: [Double]
    let png: String

    static let rows: [MascotState] = [.idle, .working, .waiting, .sleeping]

    func row(_ state: MascotState) -> Int { Self.rows.firstIndex(of: state)! }

    /// Decoded once; a sheet that fails to decode draws nothing.
    static let sheets: [String: CGImage] = Dictionary(uniqueKeysWithValues: all.compactMap { mascot in
        guard let data = Data(base64Encoded: mascot.png),
              let source = CGImageSourceCreateWithData(data as CFData, nil),
              let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else { return nil }
        return (mascot.id, image)
    })

    func frame(_ state: MascotState, tick: Int?) -> CGImage? {
        let r = row(state)
        let column = (tick ?? 0) % frames[r]
        return Self.sheets[id]?.cropping(to: CGRect(x: CGFloat(column) * cell.width, y: CGFloat(r) * cell.height,
                                                    width: cell.width, height: cell.height))
    }
}

/// Who takes turns: turn 0 is swarm's own Claude or Codex art, turn n is `SheetMascot.all[n - 1]`.
enum MascotCast {
    static var count: Int { SheetMascot.all.count + 1 }

    static func guest(_ turn: Int) -> SheetMascot? { turn > 0 ? SheetMascot.all[(turn - 1) % SheetMascot.all.count] : nil }

    static func fps(turn: Int, state: MascotState) -> Double {
        guest(turn).map { $0.fps[$0.row(state)] } ?? state.fps
    }
}

struct MascotPose {
    let sprite: Sprite
    let palette: [Character: Color]
    /// Up from the rest position, in cells.
    var bob: CGFloat = 0
}

/// The own art's frame; `tick` counts frames at the state's rate, nil is the still frame for
/// Reduce Motion and snapshots.
func mascotPose(family: String, state: MascotState, tick: Int?) -> MascotPose {
    let art = MascotArt.of(family)
    let t = tick ?? 0
    switch state {
    case .working:
        return MascotPose(sprite: art.working[t % art.working.count], palette: art.palette, bob: t % 2 == 1 ? 1 / 3 : 0)
    case .waiting:
        return MascotPose(sprite: art.waiting, palette: art.palette)
    case .idle:
        // A 1.5 s bob; a blink every 3.5 s.
        let bob: [CGFloat] = [0, 1, 2, 3, 2, 1]
        return MascotPose(sprite: t % 14 == 13 ? art.blink : art.idle, palette: art.palette, bob: bob[t % 6] / 3)
    case .sleeping:
        return MascotPose(sprite: art.sleeping, palette: art.palette)
    }
}

/// The `!` blinks once a second while an agent waits; the `z` rises a step a second, three steps,
/// then starts again. `seconds` is nil for the still frame.
func mascotCue(_ state: MascotState, seconds: Double?, ownZ: Bool) -> (glyph: Sprite, color: Color, rise: Int)? {
    switch state {
    case .waiting:
        return seconds.map { Int($0 * 2) % 2 == 0 } ?? true ? (MascotArt.bang, Palette.warn, 0) : nil
    case .sleeping:
        return ownZ ? nil : (MascotArt.zee, Palette.secondary, seconds.map { Int($0) % 3 } ?? 1)
    case .working, .idle:
        return nil
    }
}

/// Draws one frame of whoever has the turn, `height` points tall and centred; wide art shrinks to
/// the canvas width. The cue sits above the art's right edge and stays inside the canvas.
struct MascotCanvas: View {
    let family: String
    let state: MascotState
    let turn: Int
    let height: CGFloat
    let tick: Int?
    let seconds: Double?

    var body: some View {
        Canvas { context, size in
            // Whole device pixels (0.5 pt at 2x), so no edge is blurred.
            let snap = { (value: CGFloat) in (value * 2).rounded() / 2 }
            let art: CGRect
            if let guest = MascotCast.guest(turn) {
                let scale = min(height / guest.cell.height, size.width / guest.cell.width)
                let w = snap(guest.cell.width * scale), h = snap(guest.cell.height * scale)
                art = CGRect(x: snap((size.width - w) / 2), y: snap((size.height - h) / 2), width: w, height: h)
                if let frame = guest.frame(state, tick: tick) {
                    context.draw(Image(decorative: frame, scale: 1).interpolation(.none), in: art)
                }
            } else {
                let pose = mascotPose(family: family, state: state, tick: tick)
                let cell = min(snap(height / CGFloat(MascotArt.rows)), snap(size.width / CGFloat(MascotArt.columns)))
                let w = CGFloat(MascotArt.columns) * cell, h = CGFloat(MascotArt.rows) * cell
                art = CGRect(x: snap((size.width - w) / 2), y: snap((size.height - h) / 2 - pose.bob * cell), width: w, height: h)
                draw(pose.sprite, { pose.palette[$0] }, at: art.origin, cell: cell, in: context)
            }
            if let cue = mascotCue(state, seconds: seconds, ownZ: MascotCast.guest(turn)?.ownZ ?? false) {
                let cell = max((height / 24).rounded(), 1)
                let w = CGFloat(cue.glyph[0].count) * cell, h = CGFloat(cue.glyph.count) * cell
                let x = min(art.maxX - w / 2, size.width - w)
                let y = max(art.minY - h, 0) + CGFloat(2 - cue.rise) * cell
                draw(cue.glyph, { $0 == "K" ? cue.color : nil }, at: CGPoint(x: snap(x), y: snap(y)), cell: cell, in: context)
            }
        }
        .accessibilityHidden(true)
    }

    private func draw(_ sprite: Sprite, _ color: (Character) -> Color?, at origin: CGPoint, cell: CGFloat, in context: GraphicsContext) {
        for (r, line) in sprite.enumerated() {
            for (c, key) in line.enumerated() {
                guard let fill = color(key) else { continue }
                let rect = CGRect(x: origin.x + CGFloat(c) * cell, y: origin.y + CGFloat(r) * cell, width: cell, height: cell)
                context.fill(Path(rect), with: .color(fill), style: FillStyle(antialiased: false))
            }
        }
    }
}

/// Moves on a periodic clock at the art's frame rate only; with Reduce Motion it holds a still frame.
struct Mascot: View {
    let family: String
    let state: MascotState
    let turn: Int
    let height: CGFloat
    let live: Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        if live && !reduceMotion {
            let fps = MascotCast.fps(turn: turn, state: state)
            // From a fixed date, so each entry lands on a whole frame number.
            TimelineView(.periodic(from: Date(timeIntervalSinceReferenceDate: 0), by: 1 / fps)) { context in
                let seconds = context.date.timeIntervalSinceReferenceDate
                MascotCanvas(family: family, state: state, turn: turn, height: height,
                             tick: Int((seconds * fps).rounded()), seconds: seconds)
            }
            .id("\(turn)-\(state)")
        } else {
            MascotCanvas(family: family, state: state, turn: turn, height: height, tick: nil, seconds: nil)
        }
    }
}
