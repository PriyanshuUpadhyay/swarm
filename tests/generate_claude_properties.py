from pathlib import Path
s=Path('ui/Sources/SwarmCore/Composer/ClaudeInvisibleText.swift').read_text()
fields=['letter','mark','whitespace','emoji','pictographic','mathSymbol','digit','asciiDigit','rightToLeft','arabicLetterMarkScripts','variationOneScripts','brahmi']
scripts=['nonJoinerScripts','joinerScripts','cursiveScripts','southeastAsian','mongolian','khmer','egyptian','duployan']
expr= [(x.upper(), 'Sets.'+x) for x in fields]+[(x.upper()+'_'+part.upper(),'Sets.'+x+'.'+part) for x in scripts for part in ['base','mark']]
script=s+'''
var all = String.UnicodeScalarView()
var offsets: [UInt32] = []
for value in UInt32(0)...0x10FFFF {
    guard let scalar = Unicode.Scalar(value) else { continue }
    all.append(scalar)
    offsets.append(value)
    if value > 0xFFFF { offsets.append(value) }
}
let text = String(all)
var result = "// Generated from ClaudeInvisibleText.swift Foundation Unicode sets on 2026-10-13.\\n// Regenerate with tests/generate_claude_properties.py on macOS.\\n"
'''
for name,ref in expr:
 script+=f'''\ndo {{
    let pattern = {ref}.regex.pattern
    let regex = try NSRegularExpression(pattern: String(pattern.dropFirst().dropLast()))
    let matches = regex.matches(in: text, range: NSRange(location: 0, length: text.utf16.count))
    var ranges: [(UInt32, UInt32)] = []
    for match in matches {{
        let value = offsets[match.range.location]
        if let last = ranges.last, last.1 + 1 == value {{ ranges[ranges.count - 1].1 = value }}
        else {{ ranges.append((value, value)) }}
    }}
    result += "#[rustfmt::skip]\\npub(super) const {name}: &[(u32, u32)] = &[\\n"
    for range in ranges {{ result += String(format: "    (0x%X, 0x%X),\\n", range.0, range.1) }}
    result += "];\\n"
}}
'''
script+='try result.write(toFile: CommandLine.arguments[1], atomically: true, encoding: .utf8)\n'
script=script.replace('private let regex:', 'let regex:')
import subprocess, tempfile
with tempfile.TemporaryDirectory() as folder:
    file = Path(folder) / 'properties.swift'
    file.write_text(script)
    subprocess.run(['swift', '-module-cache-path', folder + '/cache', str(file), 'src/view/claude_properties.rs'], check=True)
