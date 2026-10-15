import SwarmHUDCore
import SwiftUI

// The closed wing's pixel mascot. The idea of a notch pet that works, waits and sleeps with its
// agents comes from Notchi; that project is GPL-3.0, so this art and code are our own. Each frame
// is a grid of characters, `.` empty, one colour per character (the cue glyphs use `K`), 12 cells wide and 10 tall at 1.5 pt (3 px on a Retina notch).

typealias Sprite = [String]

struct MascotArt {
    let palette: [Character: Color]
    let idle: Sprite
    let blink: Sprite
    /// Two to four frames at 4 fps.
    let working: [Sprite]
    let waiting: Sprite
    let sleeping: Sprite

    static let cell: CGFloat = 1.5
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
    /// The mascot's frame rate; nothing in the closed HUD redraws faster.
    var fps: Double {
        switch self {
        case .working, .idle: return 4
        case .waiting: return 2
        case .sleeping: return 1
        }
    }
}

struct MascotPose {
    let sprite: Sprite
    let palette: [Character: Color]
    /// Points up from the rest position.
    var bob: CGFloat = 0
    /// A glyph in one colour, placed in cells from the sprite's top-left corner.
    var cue: (glyph: Sprite, color: Color, column: Int, row: Int)?
}

/// `tick` counts frames at the state's rate; nil is the still frame for Reduce Motion and snapshots.
func mascotPose(family: String, state: MascotState, tick: Int?) -> MascotPose {
    let art = MascotArt.of(family)
    let t = tick ?? 0
    switch state {
    case .working:
        return MascotPose(sprite: art.working[t % art.working.count], palette: art.palette, bob: t % 2 == 1 ? 0.5 : 0)
    case .waiting:
        // Still; only the `!` blinks.
        return MascotPose(sprite: art.waiting, palette: art.palette,
                          cue: t % 2 == 0 ? (MascotArt.bang, Palette.warn, 12, -5) : nil)
    case .idle:
        // A 1.5 s bob in 0.5 pt (1 px) steps; a blink every 3.5 s.
        let bob: [CGFloat] = [0, 0.5, 1, 1.5, 1, 0.5]
        return MascotPose(sprite: t % 14 == 13 ? art.blink : art.idle, palette: art.palette, bob: bob[t % 6])
    case .sleeping:
        // The `z` rises a cell a second, three steps, then starts again.
        return MascotPose(sprite: art.sleeping, palette: art.palette,
                          cue: (MascotArt.zee, Palette.secondary, 10, -4 - (tick ?? 1) % 3))
    }
}

struct MascotCanvas: View {
    let pose: MascotPose

    var body: some View {
        Canvas { context, size in
            let cell = MascotArt.cell
            // Whole device pixels (0.5 pt at 2x), so no cell edge is blurred.
            let snap = { (value: CGFloat) in (value * 2).rounded() / 2 }
            let x = snap((size.width - CGFloat(MascotArt.columns) * cell) / 2)
            let y = snap((size.height - CGFloat(MascotArt.rows) * cell) / 2 - pose.bob)
            func draw(_ sprite: Sprite, _ color: (Character) -> Color?, column: Int, row: Int) {
                for (r, line) in sprite.enumerated() {
                    for (c, key) in line.enumerated() {
                        guard let fill = color(key) else { continue }
                        let rect = CGRect(x: x + CGFloat(column + c) * cell, y: y + CGFloat(row + r) * cell, width: cell, height: cell)
                        context.fill(Path(rect), with: .color(fill), style: FillStyle(antialiased: false))
                    }
                }
            }
            draw(pose.sprite, { pose.palette[$0] }, column: 0, row: 0)
            if let cue = pose.cue {
                draw(cue.glyph, { $0 == "K" ? cue.color : nil }, column: cue.column, row: cue.row)
            }
        }
        .accessibilityHidden(true)
    }
}

/// Moves on a periodic clock at the state's frame rate only; with Reduce Motion it holds a still frame.
struct Mascot: View {
    let family: String
    let state: MascotState
    let live: Bool
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        if live && !reduceMotion {
            // From a fixed date, so each entry lands on a whole frame number.
            TimelineView(.periodic(from: Date(timeIntervalSinceReferenceDate: 0), by: 1 / state.fps)) { context in
                MascotCanvas(pose: mascotPose(family: family, state: state,
                                              tick: Int((context.date.timeIntervalSinceReferenceDate * state.fps).rounded())))
            }
            .id(state)
        } else {
            MascotCanvas(pose: mascotPose(family: family, state: state, tick: nil))
        }
    }
}
