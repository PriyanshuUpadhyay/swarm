import Foundation

/// The cleaning that Claude Code 2.1.287 runs on Enter (`fue` in its binary). When it removes a
/// character, Claude puts the rest back in its box and sends nothing, so the app removes the same
/// characters first. Claude keeps some of them in their script or emoji context, which it judges
/// from the last 16 scalars it kept, so this does the same.
struct ClaudeInvisibleText {
    /// How many kept scalars judge the context; Claude's cleaner uses the same size.
    static let contextSize = 16

    static func cleaned(_ text: String) -> String {
        guard text.unicodeScalars.contains(where: { !isPlain($0) }) else { return text }
        var cleaner = ClaudeInvisibleText(scalars: Array(text.unicodeScalars))
        return cleaner.run()
    }

    private let scalars: [Unicode.Scalar]
    private var recent: [Unicode.Scalar] = []
    private var lastWasContextual = false
    private var lineStart = 0
    private var lineHasRightToLeft: Bool?
    private var lineHasArabicLetter: Bool?

    private init(scalars: [Unicode.Scalar]) {
        self.scalars = scalars
    }

    private mutating func run() -> String {
        var output = String.UnicodeScalarView()
        var index = 0
        while index < scalars.count {
            let scalar = scalars[index]
            index += 1
            let next = index < scalars.count ? scalars[index] : nil
            if !Self.isCandidate(scalar) {
                if scalar == "\n" { startLine(at: index) }
                output.append(scalar)
                remember(scalar, contextual: false)
                if scalar.value == 0x1F3F4,
                   let tags = Self.subdivisionTags.first(where: { scalars[index...].starts(with: $0) }) {
                    for tag in tags {
                        output.append(tag)
                        remember(tag, contextual: true)
                    }
                    index += tags.count
                }
            } else if Self.isLineBreak(scalar) {
                // The CR of a CRLF goes, and the LF after it ends the line.
                if scalar != "\r" || next != "\n" {
                    output.append("\n")
                    remember("\n", contextual: false)
                    startLine(at: index)
                }
            } else if keeps(scalar, next: next) {
                output.append(scalar)
                remember(scalar, contextual: true)
            }
        }
        return String(output)
    }

    private mutating func keeps(_ scalar: Unicode.Scalar, next: Unicode.Scalar?) -> Bool {
        let last = recent.last
        let afterContextual = lastWasContextual
        switch scalar.value {
        case 0x200C:
            if follows(Sets.nonJoinerScripts) { return true }
            guard let last, !afterContextual else { return false }
            return Self.isLetter(next, of: Sets.nonJoinerScripts)
                && !Sets.whitespace.contains(last) && !Sets.mark.contains(last)
        case 0x200D:
            if follows(Sets.joinerScripts) { return true }
            if let last, last.value != 0x200D, Sets.pictographic.contains(emojiBeforeModifiers()),
               Self.isKept(next, in: Sets.pictographic) { return true }
            guard let last, !afterContextual else { return false }
            return Self.isLetter(next, of: Sets.cursiveScripts) && !Sets.letter.contains(last)
                && !Sets.digit.contains(last) && !Sets.mark.contains(last)
        case 0x200B:
            let nextIsSoutheastAsian = Self.isKept(next, in: Sets.southeastAsian.base)
                && !Sets.mark.contains(next)
            return follows(Sets.southeastAsian, anyBase: true)
                && (nextIsSoutheastAsian || Self.isKept(next, in: Sets.asciiDigit))
                || nextIsSoutheastAsian && lastIs(Sets.asciiDigit)
        case 0xFE0E, 0xFE0F:
            guard let last, !afterContextual else { return false }
            return last.value >= 0xA9 && Sets.emoji.contains(last)
                || "0123456789#*".unicodeScalars.contains(last) && next?.value == 0x20E3
        case 0xFE00...0xFE02:
            guard let last, !afterContextual else { return false }
            return Sets.egyptian.base.contains(last) || scalar.value == 0xFE00
                && ((0x2200...0x2AFF).contains(last.value) && Sets.mathSymbol.contains(last)
                    || Sets.variationOneScripts.contains(last))
        case 0x200E, 0x200F, 0x061C:
            let lineFits = scalar.value == 0x061C
                ? lineHas(Sets.arabicLetterMarkScripts, cache: &lineHasArabicLetter)
                : lineHas(Sets.rightToLeft, cache: &lineHasRightToLeft)
            guard lineFits, !afterContextual else { return false }
            let nextFits = next.map {
                Self.isLineBreak($0) || !Self.isCandidate($0) && !Sets.mark.contains($0)
            } ?? true
            return nextFits && !continuesDirection(into: next)
        case 0x034F:
            return lastIs(Sets.mark) || !afterContextual && Self.isKept(next, in: Sets.mark)
        case 0x17B4, 0x17B5:
            return follows(Sets.khmer)
        case 0x180B...0x180F:
            return follows(Sets.mongolian) || !afterContextual && Self.isLetter(next, of: Sets.mongolian)
        case 0x1107F:
            return lastIs(Sets.brahmi) && Self.isKept(next, in: Sets.brahmi)
        case 0x13430...0x1343F:
            return follows(Sets.egyptian) || !afterContextual && Self.isLetter(next, of: Sets.egyptian)
        case 0x1BCA0...0x1BCA3:
            return follows(Sets.duployan) || !afterContextual && Self.isLetter(next, of: Sets.duployan)
        default:
            return false
        }
    }

    private mutating func remember(_ scalar: Unicode.Scalar, contextual: Bool) {
        recent.append(scalar)
        if recent.count > Self.contextSize { recent.removeFirst() }
        lastWasContextual = contextual
    }

    private mutating func startLine(at index: Int) {
        lineStart = index
        lineHasRightToLeft = nil
        lineHasArabicLetter = nil
    }

    /// Whether the line holds a right-to-left letter of the set, before or after this point.
    private func lineHas(_ set: ScalarSet, cache: inout Bool?) -> Bool {
        if let cache { return cache }
        var found = false
        for scalar in scalars[lineStart...] {
            if Self.isLineBreak(scalar) { break }
            if Self.isRightToLeftBlock(scalar), Sets.letter.contains(scalar), set.contains(scalar) {
                found = true
                break
            }
        }
        cache = found
        return found
    }

    /// Whether the last kept base, past any marks of the script, is in the script; a letter
    /// unless `anyBase`.
    private func follows(_ script: ScriptSet, anyBase: Bool = false) -> Bool {
        guard !lastWasContextual else { return false }
        for scalar in recent.reversed() {
            if Sets.mark.contains(scalar) {
                if !script.mark.contains(scalar) { return false }
                continue
            }
            return script.base.contains(scalar) && (anyBase || Sets.letter.contains(scalar))
        }
        return false
    }

    private func lastIs(_ set: ScalarSet) -> Bool {
        !lastWasContextual && set.contains(recent.last)
    }

    /// The emoji a ZWJ would join, past one or two VS16 or skin tone scalars.
    private func emojiBeforeModifiers() -> Unicode.Scalar? {
        var offset = 0
        while offset < 2, let scalar = recent.dropLast(offset).last,
              scalar.value == 0xFE0F || (0x1F3FB...0x1F3FF).contains(scalar.value) {
            offset += 1
        }
        return recent.dropLast(offset).last
    }

    /// Whether a direction mark sits between two letters of one direction or two digits, so it
    /// changes nothing.
    private func continuesDirection(into next: Unicode.Scalar?) -> Bool {
        guard let next else { return false }
        let nextIsLetter = Sets.letter.contains(next)
        guard nextIsLetter || Sets.digit.contains(next) else { return false }
        for scalar in recent.reversed() {
            if Self.isCandidate(scalar) { return true }
            if Sets.mark.contains(scalar) { continue }
            return nextIsLetter
                ? Sets.letter.contains(scalar) && Self.isRightToLeft(scalar) == Self.isRightToLeft(next)
                : Sets.digit.contains(scalar)
        }
        return recent.count >= Self.contextSize
    }

    private static let subdivisionTags: [[Unicode.Scalar]] = ["gbeng", "gbsct", "gbwls"].map { name in
        name.unicodeScalars.compactMap { Unicode.Scalar($0.value + 0xE0000) } + ["\u{E007F}"]
    }

    private static func isPlain(_ scalar: Unicode.Scalar) -> Bool {
        scalar == "\t" || scalar == "\n" || (0x20...0x7E).contains(scalar.value)
    }

    /// The scalars Claude removes unless a context rule in `keeps` holds.
    private static func isCandidate(_ scalar: Unicode.Scalar) -> Bool {
        switch scalar.value {
        case 0x00...0x08, 0x0B...0x1F, 0x7F...0x9F, 0xAD, 0x34F, 0x61C, 0x115F, 0x1160, 0x17B4,
             0x17B5, 0x180B...0x180F, 0x200B...0x200F, 0x2028...0x202E, 0x2060...0x206F, 0x3164,
             0xFE00...0xFE0F, 0xFEFF, 0xFFA0, 0xFFF0...0xFFFB, 0x1107F, 0x13430...0x1343F, 0x16FE4,
             0x1BCA0...0x1BCA3, 0x1D173...0x1D17A, 0xE0000...0xE0FFF:
            true
        default:
            false
        }
    }

    private static func isLineBreak(_ scalar: Unicode.Scalar) -> Bool {
        switch scalar.value {
        case 0x0A...0x0D, 0x85, 0x2028, 0x2029: true
        default: false
        }
    }

    private static func isRightToLeftBlock(_ scalar: Unicode.Scalar) -> Bool {
        switch scalar.value {
        case 0x0590...0x08FF, 0xFB1D...0xFDFF, 0xFE70...0xFEFF, 0x10800...0x10FFF, 0x1E800...0x1EFFF:
            true
        default:
            false
        }
    }

    private static func isRightToLeft(_ scalar: Unicode.Scalar) -> Bool {
        scalar.value == 0x0640 || Sets.rightToLeft.contains(scalar)
    }

    private static func isKept(_ scalar: Unicode.Scalar?, in set: ScalarSet) -> Bool {
        guard let scalar else { return false }
        return !isCandidate(scalar) && set.contains(scalar)
    }

    private static func isLetter(_ scalar: Unicode.Scalar?, of script: ScriptSet) -> Bool {
        isKept(scalar, in: script.base) && Sets.letter.contains(scalar)
    }
}

private struct ScalarSet: Sendable {
    private let regex: NSRegularExpression

    init(_ classBody: String) {
        regex = try! NSRegularExpression(pattern: "^[\(classBody)]$")
    }

    init(scripts: String, property: String = "Script") {
        self.init(scripts.split(separator: " ").map { "\\p{\(property)=\($0)}" }.joined())
    }

    func contains(_ scalar: Unicode.Scalar?) -> Bool {
        guard let scalar else { return false }
        let text = String(scalar)
        return regex.firstMatch(in: text, range: NSRange(location: 0, length: text.utf16.count)) != nil
    }
}

/// A script's letters, and the marks it shares with other scripts.
private struct ScriptSet: Sendable {
    let base: ScalarSet
    let mark: ScalarSet

    init(scripts: String) {
        base = ScalarSet(scripts: scripts)
        mark = ScalarSet(scripts: scripts, property: "Script_Extensions")
    }

    init(only set: ScalarSet) {
        base = set
        mark = set
    }
}

private enum Sets {
    static let letter = ScalarSet(#"\p{L}"#)
    static let mark = ScalarSet(#"\p{M}"#)
    static let whitespace = ScalarSet(#"\p{White_Space}"#)
    static let emoji = ScalarSet(#"\p{Emoji}\p{Extended_Pictographic}"#)
    static let pictographic = ScalarSet(#"\p{Extended_Pictographic}"#)
    static let mathSymbol = ScalarSet(#"\p{Sm}"#)
    static let digit = ScalarSet(#"\p{Nd}"#)
    static let asciiDigit = ScalarSet("0-9")
    static let rightToLeft = ScalarSet(
        scripts: "Arabic Hebrew Syriac Thaana Nko Samaritan Mandaic Adlam Hanifi_Rohingya Yezidi"
    )
    static let arabicLetterMarkScripts = ScalarSet(scripts: "Arabic Syriac Thaana Hanifi_Rohingya")
    static let nonJoinerScripts = ScriptSet(
        scripts: "Arabic Syriac Nko Mongolian Devanagari Bengali Gurmukhi Gujarati Oriya Tamil Telugu"
            + " Kannada Malayalam Sinhala Myanmar Khmer Tibetan"
    )
    static let joinerScripts = ScriptSet(
        scripts: "Devanagari Bengali Gurmukhi Gujarati Oriya Tamil Telugu Kannada Malayalam Sinhala"
            + " Myanmar Khmer Tibetan Arabic Syriac Tifinagh"
    )
    static let cursiveScripts = ScriptSet(scripts: "Arabic Syriac Mongolian Nko")
    static let southeastAsian = ScriptSet(only: ScalarSet(scripts: "Khmer Thai Lao Myanmar"))
    static let variationOneScripts = ScalarSet(scripts: "Myanmar Phags_Pa Manichaean")
    static let mongolian = ScriptSet(only: ScalarSet(scripts: "Mongolian"))
    static let khmer = ScriptSet(only: ScalarSet(scripts: "Khmer"))
    static let egyptian = ScriptSet(only: ScalarSet(scripts: "Egyptian_Hieroglyphs"))
    static let duployan = ScriptSet(only: ScalarSet(scripts: "Duployan"))
    static let brahmi = ScalarSet(scripts: "Brahmi")
}
