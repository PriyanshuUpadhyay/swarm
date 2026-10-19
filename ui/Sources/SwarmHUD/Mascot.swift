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

    static func of(_ family: String) -> MascotArt {
        switch family {
        case "codex": return codex
        case "agy": return agy
        case "pi": return pi
        case "bee": return bee
        default: return claude
        }
    }

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

    /// Swarm's bee, side on, facing right; it takes its own turn in the cast. Y body, D stripe,
    /// W wing, E eye, K sting and legs.
    static let bee = MascotArt(
        palette: [
            "Y": Color(hex: 0xF2C230), "D": Color(hex: 0x2B2420), "W": Color(hex: 0xD8E8FA),
            "E": Color(hex: 0x15110F), "K": Color(hex: 0x4A3F38),
        ],
        idle: [
            "....WW.WW...",
            "...WWWWWWW..",
            "....WWWWW...",
            "..YDDYDDYY..",
            ".YYDDYDDYEY.",
            "KYYDDYDDYYY.",
            ".YYDDYDDYYY.",
            "..YDDYDDYY..",
            "...K....K...",
            "............",
        ],
        blink: [
            "....WW.WW...",
            "...WWWWWWW..",
            "....WWWWW...",
            "..YDDYDDYY..",
            ".YYDDYDDYYY.",
            "KYYDDYDDYYY.",
            ".YYDDYDDYYY.",
            "..YDDYDDYY..",
            "...K....K...",
            "............",
        ],
        working: [
            [
                "..WWW..WWW..",
                "...WWW.WWW..",
                "....WWWWW...",
                "..YDDYDDYY..",
                ".YYDDYDDYEY.",
                "KYYDDYDDYYY.",
                ".YYDDYDDYYY.",
                "..YDDYDDYY..",
                "...K....K...",
                "............",
            ],
            [
                "............",
                "............",
                "..WWWWWWWW..",
                ".WYDDYDDYYW.",
                ".YYDDYDDYEY.",
                "KYYDDYDDYYY.",
                ".YYDDYDDYYY.",
                "..YDDYDDYY..",
                "...K....K...",
                "............",
            ],
        ],
        waiting: [
            "....WW.WW...",
            "...WWWWWWW..",
            "....WWWWW...",
            "..YDDYDDYY..",
            ".YYDDYDDYEY.",
            "KYYDDYDDYYY.",
            ".YYDDYDDYYY.",
            "..YDDYDDYY..",
            "..K......K..",
            "............",
        ],
        sleeping: [
            "............",
            "............",
            "............",
            "...WWWWWW...",
            "..YDDYDDYY..",
            ".YYDDYDDYDY.",
            "KYYDDYDDYYY.",
            ".YYDDYDDYYY.",
            "..KKKKKKKK..",
            "............",
        ]
    )

    /// pi's mascot: a π that walks on its two legs. B body, S shade, W eye shine, E eye.
    static let pi = MascotArt(
        palette: [
            "B": Color(hex: 0x9A7BD8), "S": Color(hex: 0x6E54A8), "W": Color(hex: 0xF4EEFF),
            "E": Color(hex: 0x1C1530),
        ],
        idle: [
            "............",
            ".BBBBBBBBBB.",
            "BBWEBBBBWEBB",
            "BBEEBBBBEEBB",
            ".SSBSSSSBSS.",
            "...BB...BB..",
            "...BB...BB..",
            "...BB...BB..",
            "...BB...BBB.",
            "..BB.....BB.",
        ],
        blink: [
            "............",
            ".BBBBBBBBBB.",
            "BBBBBBBBBBBB",
            "BBEEBBBBEEBB",
            ".SSBSSSSBSS.",
            "...BB...BB..",
            "...BB...BB..",
            "...BB...BB..",
            "...BB...BBB.",
            "..BB.....BB.",
        ],
        working: [
            [
                "............",
                ".BBBBBBBBBB.",
                "BBWEBBBBWEBB",
                "BBEEBBBBEEBB",
                ".SSBSSSSBSS.",
                "...BB...BB..",
                "...BB...BB..",
                "..BB.....BB.",
                ".BB.......BB",
                ".B.........B",
            ],
            [
                "............",
                ".BBBBBBBBBB.",
                "BBWEBBBBWEBB",
                "BBEEBBBBEEBB",
                ".SSBSSSSBSS.",
                "...BB...BB..",
                "...BB...BB..",
                "....BB.BB...",
                "....BB.BB...",
                "...BB...BB..",
            ],
        ],
        waiting: [
            "............",
            ".BBBBBBBBBB.",
            "BBWEBBBBWEBB",
            "BBEEBBBBEEBB",
            ".SSBSSSSBSS.",
            "...BB...BB..",
            "...BB...BBBB",
            "...BB.......",
            "...BB.......",
            "..BB........",
        ],
        sleeping: [
            "............",
            "............",
            "............",
            "............",
            "............",
            ".BBBBBBBBBB.",
            "BBBBBBBBBBBB",
            "BBEEBBBBEEBB",
            ".SSBSSSSBSS.",
            "..BBB..BBB..",
        ]
    )

    /// AGY's mascot: a big star with a face and its small twin. B and P stars, b and p dimmed,
    /// E eye.
    static let agy = MascotArt(
        palette: [
            "B": Color(hex: 0x7EA6FF), "P": Color(hex: 0xE08CF2), "b": Color(hex: 0x4A5F94),
            "p": Color(hex: 0x7E5689), "E": Color(hex: 0x141A2C),
        ],
        idle: [
            "............",
            "....B.....P.",
            "....B....PPP",
            "...BBB....P.",
            "..BEBEB.....",
            "BBBBBBBBB...",
            "..BBBBB.....",
            "...BBB......",
            "....B.......",
            "....B.......",
        ],
        blink: [
            "............",
            "....B.....P.",
            "....B....PPP",
            "...BBB....P.",
            "..BBBBB.....",
            "BBBBBBBBB...",
            "..BBBBB.....",
            "...BBB......",
            "....B.......",
            "....B.......",
        ],
        working: [
            [
                "............",
                "....B.....P.",
                "....B....PPP",
                "...BBB....P.",
                "..BEBEB.....",
                "BBBBBBBBB...",
                "..BBBBB.....",
                "...BBB......",
                "....B.......",
                "....B.......",
            ],
            [
                "............",
                "............",
                "....B.......",
                "...BBB......",
                "..BEBEB.....",
                ".BBBBBBB....",
                "..BBBBB...P.",
                "...BBB...PPP",
                "....B.....P.",
                "............",
            ],
        ],
        waiting: [
            "............",
            "....B.....p.",
            "....B....ppp",
            "...BBB....p.",
            "..BEBEB.....",
            "BBBBBBBBB...",
            "..BBBBB.....",
            "...BBB......",
            "....B.......",
            "....B.......",
        ],
        sleeping: [
            "............",
            "............",
            "....b.......",
            "...bbb......",
            "..bbbbb.....",
            ".bbbbbbb....",
            "..bEbEb.....",
            "...bbb...p..",
            "....b...ppp.",
            ".........p..",
        ]
    )

    static let bang: Sprite = ["K", "K", "K", ".", "K"]
    static let drop: Sprite = [".K.", ".K.", "KKK", "KKK", ".K."]
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

/// A built-in guest mascot: a PNG sheet with a row per state and a column per frame
/// (`MascotSheets.swift`).
struct SheetMascot {
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
}

/// A mascot drawn from a decoded sprite sheet: a built-in guest or one of the owner's Codex pets.
struct Guest {
    let name: String
    let credit: String
    /// The art draws its own sleep, so the HUD adds no `z`.
    let ownZ: Bool
    /// Painted art (a Codex pet) scales smoothly; pixel art keeps hard edges.
    let smooth: Bool
    /// One frame, in sheet pixels.
    let cell: CGSize
    let columns: Int
    let sheet: CGImage
    let animations: [MascotState: CodexPetManifest.Animation]

    func fps(_ state: MascotState) -> Double { animations[state]?.fps ?? 1 }

    func frame(_ state: MascotState, tick: Int?) -> CGImage? {
        guard let frames = animations[state]?.frames, !frames.isEmpty else { return nil }
        let index = frames[(tick ?? 0) % frames.count]
        return sheet.cropping(to: CGRect(x: CGFloat(index % columns) * cell.width, y: CGFloat(index / columns) * cell.height,
                                         width: cell.width, height: cell.height))
    }
}

extension Guest {
    /// A sheet that fails to decode leaves the guest out.
    init?(_ mascot: SheetMascot) {
        guard let data = Data(base64Encoded: mascot.png),
              let source = CGImageSourceCreateWithData(data as CFData, nil),
              let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else { return nil }
        let columns = mascot.frames.max() ?? 1
        var animations: [MascotState: CodexPetManifest.Animation] = [:]
        for (row, state) in SheetMascot.rows.enumerated() {
            animations[state] = .init(frames: (0..<mascot.frames[row]).map { row * columns + $0 }, fps: mascot.fps[row])
        }
        self.init(name: mascot.name, credit: mascot.credit, ownZ: mascot.ownZ, smooth: false, cell: mascot.cell,
                  columns: columns, sheet: image, animations: animations)
    }

    /// Each decoded sheet stays in memory, so the cast takes at most this many Codex pets, each at
    /// most 4096 x 4096 pixels (Codex's own sheet is 1536 x 1872).
    static let maxCodexPets = 16, maxSheetSide = 4096

    /// The pets the owner installed for Codex, `<home>/pets/<id>/pet.json`, by folder name. A pet
    /// whose manifest or sheet does not read, whose sheet is too big, or whose sheet is smaller than
    /// its grid, is left out with a line on stderr.
    static func codexPets(home: URL) -> [Guest] {
        let pets = home.appendingPathComponent("pets")
        let ids = (try? FileManager.default.contentsOfDirectory(atPath: pets.path))?.sorted() ?? []
        return Array(ids.compactMap { id -> Guest? in
            let folder = pets.appendingPathComponent(id)
            let skip = { (why: String) -> Guest? in
                FileHandle.standardError.write(Data("SwarmHUD: Codex pet \(id) left out: \(why)\n".utf8))
                return nil
            }
            guard let data = try? Data(contentsOf: folder.appendingPathComponent("pet.json")) else { return nil }
            let pet: CodexPetManifest
            do { pet = try CodexPetManifest.parse(data, folder: id) } catch { return skip("\(error)") }
            guard let source = CGImageSourceCreateWithURL(folder.appendingPathComponent(pet.sheetPath) as CFURL, nil),
                  let size = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
                  let width = size[kCGImagePropertyPixelWidth] as? Int, let height = size[kCGImagePropertyPixelHeight] as? Int
            else { return skip("\(pet.sheetPath) does not read as an image") }
            guard width <= maxSheetSide, height <= maxSheetSide else { return skip("sheet is \(width) x \(height)") }
            guard width >= pet.frame.width * pet.frame.columns, height >= pet.frame.height * pet.frame.rows,
                  let sheet = CGImageSourceCreateImageAtIndex(source, 0, nil)
            else { return skip("sheet is smaller than its frame grid") }
            return Guest(name: pet.name, credit: "Your Codex pet", ownZ: false, smooth: true,
                         cell: CGSize(width: pet.frame.width, height: pet.frame.height), columns: pet.frame.columns,
                         sheet: sheet, animations: pet.animations)
        }.prefix(maxCodexPets))
    }
}

/// Who takes turns: turn 0 is the busiest provider's own art, turn 1 swarm's bee, then the built-in
/// guests, then the owner's Codex pets.
enum MascotCast {
    static let beeTurn = 1
    /// Turns of swarm's own art before the first guest: the provider family and the bee.
    static let ownTurns = 2

    /// Read once; the HUD warms it off the main thread at launch (`AppDelegate`), and a pet
    /// installed later joins at the next launch.
    static let guests: [Guest] = SheetMascot.all.compactMap(Guest.init) + Guest.codexPets(home: codexHome)

    static var codexHome: URL {
        ProcessInfo.processInfo.environment["CODEX_HOME"].map { URL(fileURLWithPath: $0) }
            ?? FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".codex")
    }

    static var count: Int { guests.count + ownTurns }

    static func guest(_ turn: Int) -> Guest? {
        turn >= ownTurns && !guests.isEmpty ? guests[(turn - ownTurns) % guests.count] : nil
    }

    /// The own art that draws `turn` when it is not a guest's.
    static func family(_ family: String, turn: Int) -> String { turn == beeTurn ? "bee" : family }

    static func name(_ turn: Int) -> String { guest(turn)?.name ?? (turn == beeTurn ? "Swarm bee" : "Swarm") }

    static func fps(turn: Int, state: MascotState) -> Double { guest(turn)?.fps(state) ?? state.fps }
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
/// then starts again; a sweat drop falls a step a second, two steps, while a strained pet works or
/// idles. `seconds` is nil for the still frame.
func mascotCue(_ state: MascotState, seconds: Double?, ownZ: Bool, strained: Bool = false)
    -> (glyph: Sprite, color: Color, rise: Int, left: Bool)? {
    switch state {
    case .waiting:
        return seconds.map { Int($0 * 2) % 2 == 0 } ?? true ? (MascotArt.bang, Palette.warn, 0, false) : nil
    case .sleeping:
        return ownZ ? nil : (MascotArt.zee, Palette.secondary, seconds.map { Int($0) % 3 } ?? 1, false)
    case .working, .idle:
        return strained ? (MascotArt.drop, Palette.sweat, 2 - (seconds.map { Int($0) % 2 } ?? 0), true) : nil
    }
}

/// Draws one frame of whoever has the turn, `height` points tall and centred; wide art shrinks to
/// the canvas width. The cue sits above the art, at its right edge or, for the sweat drop, its left
/// edge, and stays inside the canvas. The bee
/// works in a swarm of `crowd` bees, up to 3.
struct MascotCanvas: View {
    let family: String
    let state: MascotState
    let turn: Int
    let height: CGFloat
    let tick: Int?
    let seconds: Double?
    var strained = false
    var crowd = 1

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
                    context.draw(Image(decorative: frame, scale: 1).interpolation(guest.smooth ? .high : .none), in: art)
                }
            } else {
                let own = MascotCast.family(family, turn: turn)
                let bees = own == "bee" && state == .working ? min(max(crowd, 1), 3) : 1
                let cell = min(snap(height / CGFloat(MascotArt.rows)), snap(size.width / CGFloat(MascotArt.columns * bees)))
                let w = CGFloat(MascotArt.columns) * cell, h = CGFloat(MascotArt.rows) * cell
                let left = snap((size.width - w * CGFloat(bees)) / 2)
                var frame = CGRect.null
                for bee in 0..<bees {
                    // Each bee beats its wings a frame apart, the middle one a little higher.
                    let pose = mascotPose(family: own, state: state, tick: tick.map { $0 + bee })
                    // Only as high as the free space above the art, so no wing leaves the canvas.
                    let lift = bees > 1 && bee == 1 ? min(cell * 2, max((size.height - h) / 2 - pose.bob * cell, 0)) : 0
                    let rect = CGRect(x: left + CGFloat(bee) * w, y: snap((size.height - h) / 2 - pose.bob * cell - lift), width: w, height: h)
                    draw(pose.sprite, { pose.palette[$0] }, at: rect.origin, cell: cell, in: context)
                    frame = frame.union(rect)
                }
                art = frame
            }
            if let cue = mascotCue(state, seconds: seconds, ownZ: MascotCast.guest(turn)?.ownZ ?? false, strained: strained) {
                let cell = max((height / 24).rounded(), 1)
                let w = CGFloat(cue.glyph[0].count) * cell, h = CGFloat(cue.glyph.count) * cell
                let x = cue.left ? max(art.minX - w / 2, 0) : min(art.maxX - w / 2, size.width - w)
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
    var strained = false
    var crowd = 1
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        if live && !reduceMotion {
            let fps = MascotCast.fps(turn: turn, state: state)
            // From a fixed date, so each entry lands on a whole frame number.
            TimelineView(.periodic(from: Date(timeIntervalSinceReferenceDate: 0), by: 1 / fps)) { context in
                let seconds = context.date.timeIntervalSinceReferenceDate
                MascotCanvas(family: family, state: state, turn: turn, height: height,
                             tick: Int((seconds * fps).rounded()), seconds: seconds, strained: strained, crowd: crowd)
            }
            .id("\(turn)-\(state)")
        } else {
            MascotCanvas(family: family, state: state, turn: turn, height: height, tick: nil, seconds: nil, strained: strained, crowd: crowd)
        }
    }
}
