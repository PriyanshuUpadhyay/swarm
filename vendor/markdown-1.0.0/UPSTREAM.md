# This tree maintains markdown 1.0.0 with a bounded destination patch.

The source is the published crate from
https://static.crates.io/crates/markdown/markdown-1.0.0.crate.
The archive has 219012 bytes and SHA256
`a5cab8f2cadc416a82d2e783a1946388b31654d391d1c7d92cc1f03e295b1deb`.
Its upstream repository is https://github.com/wooorm/markdown-rs.
The crate's `.cargo_vcs_info.json` records the upstream source revision.

`SHA256SUMS` records each ORIGINAL archive path and SHA256. It remains unchanged.
The local changes below are reviewed fork patches, not original archive bytes.

The original `license` contains the full MIT license and Titus Wormer notice.
Its SHA256 is
`e620971c29d750df627f71f4e629618ef8fcbbea134ab95b344fc0c1a757cad1`.
Keep it with this source and with any future source distribution.

This repository's maintainers own any later reviewed extension.
They must track local changes, retain the source and license pins, review upstream
changes on rebase, and rerun independent OLD comparisons through the real parser.
The original checksum manifest remains a record of the unpatched source.
A rebase requires a new archive pin and an explicit account of every local patch.
This contract does not promise maintenance by the upstream author.

The owner approved an exception for the pinned upstream parser file sizes,
including later reviewed patches. New project code keeps the normal size limit.
The isolated trial uses only the crate's default feature set.
App adoption and parser behavior changes require separate release by the parent.

The released inert core V2 adds optional mdast destination normalization and validation
hooks, both None by default. Only successful resource, definition, and angle exits
retain one immutable normalized href. Existing grammar failure routes own rejection.
A local String tokenizer decodes resource and definition values with the existing
escape and full HTML5 lookup. Its private numeric mode follows MarkdownIt 15.0.2
unescapeAll and entities 8.1.0 decodeHTMLStrict. Ordinary text/title String parsing
and the to_html compiler are unchanged. Event constructor additions are inert None.
The standalone trial owns the URL port; no app caller enables this extension.

Rebase duty includes reviewing all changed constructor sites, bounded String
capture, local numeric limits, rollback, retained-value consumption, None defaults
and Debug, and rerunning the frozen ORIGINAL destination tests and full default
Node/config differential. This first core leaves B2 Q1/Q2, B3 grammar/control/depth,
extra OLD entity names, NUL outside href copies, angle display text, bare links,
word/hunk/DOM/UTF16/source mapping and public adoption gates OPEN.

The bounded grammar V1 patch uses the same optional hook gate. It rejects shortcut
fallback only at EOF before a resource destination starts, admits escaped non-line
controls and DEL in raw destinations, drains escaped TAB virtual steps, caps raw
definition nesting at the existing resource limit, and admits DEL and NUL in angle
URL bodies. Source bytes and points stay under tokenizer control. The retained
href still uses the existing policy hooks; validate-only NUL keeps its raw value.
Both-None grammar stays unchanged. Rebase review must check the private EOF flag
through attempt rollback and label cleanup, TAB boundaries, hook rejection and
retained event ownership. Rerun the frozen four grammar groups, old destination
tests, and both full default Node/config captures. Q1 fallback positions, linked
String line continuations, NUL display text, and the other gates above remain OPEN
at that grammar unit's boundary.

The private linked String V2 patch additionally requires an mdast caller and at
least one URL hook. Resource destinations can continue after escaped physical
line endings through their linked String pieces. Definition destinations keep
their physical line bound; an available escaped terminal LF enters the decoded
value without taking the next line or its title. A shared pure fold preserves
owned whitespace and maps physical CR/CRLF to LF before reference decoding.
Definition titles alone omit continuation indentation. Selected title chains use
the existing local numeric compatibility mode. Their resolved child events are
folded before container reinsertion. A private title map moves each value through
Subresult and ParseState into the mdast compiler, keyed by its title-string
Enter's source byte index. The compiler removes that value into the existing
title buffer and bypasses the global reference decoder. The owning title exit still
runs once. Both-None options and all HTML routes keep their existing behavior,
including the earlier Some-hook grammar. Public options and signatures do not
change, and the standalone trial remains the only caller.

The private V3 producer repairs preserve resource title Data inside its existing
ResourceTitleString range before Text whitespace trimming. This guard uses the
same mdast and URL-hook gate; other String ranges keep their existing trim.
Enclosed resource destinations after escaped LF use the standard linked
SpaceOrTab helper for owned continuation whitespace before the next Data piece.
No global link-index or off-mode failure behavior changes.

Rebase review must check the private mdast context, linked feed and container
skips, nonempty Data boundaries, definition completion flag cleanup, shared fold,
title mode, all three title-map aggregation paths, stable source byte keys and
buffer/cursor balance, the gated resource-title trim guard and linked enclosed
continuation whitespace. Rerun the frozen linked String groups,
HTML scope guard, old destination/grammar groups and full None/default captures.
Keep the three captured per-input None and HTML outcomes unchanged.
Q1, NUL display text, full identifier normalization, renderer parity, public
adoption and the other unselected gates remain OPEN.

The coordinated identifier-separator patch is the first local patch that
intentionally changes default/None and native HTML outcomes. Earlier preservation
statements apply to their own destination, grammar and linked String patches.
The shared normalizer now retains the first internal ASCII space when its input
is nonempty, starts outside TAB, LF, CR and SPACE, and contains at least two runs
outside those four bytes. Its loop, slices, capacity and lower-then-upper case
tail stay unchanged. Restored separators split formerly merged identities, such
as `a b` and `ab`, and merge formerly separate identities, such as `a b` and
` a b`. Existing public lowercase context, including final Sigma, can also change.
Only the resulting normalized identities and existing lookups may differ; no new
panic, error or failure status is allowed in any mode.

After the unchanged true-footnote suffix lookup misses, the existing undefined
footnote fallback now normalizes the whole raw caret label from its marker to
the closing bracket. Parser admission and both compilers therefore use the same
source range. An isolated separator repair could instead cause a missing HTML
lookup and panic, or lose a valid fallback match; these are source predictions,
not executed isolated-patch results. Rebase review must compare both the upstream
normalizer and fallback construction, preserve this same-source key agreement,
and rerun the frozen public-API separator regression and GFM safety guard. Keep
the ORIGINAL `SHA256SUMS`, archive/source pins, license and earlier patch history
unchanged, including the original normalizer checksum
`cd881e566b346de5b2f4b41f30c7acae24ccd4e7827ab059de736c323837f302`.
Full identifier normalization, Q1, renderer parity, public adoption and the other
unselected gates remain OPEN. This key agreement does not prove full GFM or
JavaScript parity.

The ES-whitespace reference patch applies only when `ParseState::link_compat()`
is true (an mdast caller with at least one URL hook) and
`constructs.gfm_label_start_footnote` is off. It folds stored native keys by
trimming and collapsing exactly W25, the Node 22.23.2 single-scalar `/\s/u` set
recorded in `unit12/data-facts/report.md`: U+0009..U+000D, U+0020, U+00A0,
U+1680, U+2000..U+200A, U+2028, U+2029, U+202F, U+205F, U+3000 and U+FEFF.
The fold adds no case conversion or decoder. Ordinary references with a nonempty
fold can match definitions by that fold. Every eligible LinkReference, including
an exact hit, binds to the existing identifier of the source-first definition
with the same fold. The compiler builds this map for one compile call only.
Images, empty folds, footnote-on callers, None and native HTML keep exact matching.
An empty fold stays outside the map; a map miss keeps the current identifier.
Rebase review must keep both parser membership sites and compiler binding in
step, preserve the gate and source-first order, and rerun the frozen regression
and owner-gate guard. The empty-fold definition grammar (E1), Rust versus JS case
tables (E2), NUL versus U+FFFD (E3), whole-string `/\s+/g` and `trim()` behavior,
and the shipped browser engine's Unicode version remain OPEN. Q1 and lookup cost
also remain OPEN: a miss scans stored keys, with no cache, cap or deadline and no
performance claim. This patch does not widen the earlier ASCII-separator
exception or close renderer parity or public adoption gates.

The Unit12c patch follows `EMPTY-LABEL-V3.md` under the same gate. A definition
whose normalized label has an empty W25 fold fails and stays paragraph text.
`parser.rs` builds one nonempty folded-key set per parse, and both `label_end.rs`
sites use it for fallback membership. This supersedes the Unit12b sentences that
empty folds keep exact matching, E1 grammar remains OPEN, and a miss scans stored
keys with no cache. Rebase review must keep the `definition.rs` rejection and
`parser.rs` set in step with both `label_end.rs` sites and compiler binding,
preserve the gate and source-first order, and rerun the frozen regressions and
owner-gate guard. Other OPEN items and the exact-key lookup stay unchanged; no
general performance claim is made.

The G4 fence-word patch uses the existing mdast URL-hook gate
(`ParseState::link_compat()`). The compiler reads the raw opening fence line from
the info start through its meta, folds it with the existing ES-whitespace helper,
and keeps the first word as `Code.lang`, or None for an empty word. It preserves
entities and backslash escapes in that word. This follows the OLD fence label
`info.trim().split(/\s+/)[0]`; it does not change `Code.meta` or vendor default
behavior. Rebase review must preserve the gate, raw range, shared whitespace set,
and buffer balance, rerun the frozen `fence_word` public-API regression, and compare
default `Code.lang` for its nine inputs against the base. Fence markup, the diff
route, renderer parity and public adoption remain OPEN.

The local numeric code derives from MarkdownIt 15.0.2 dist/markdown-it.cjs.js
(isValidEntityCode and unescapeAll), and entities 8.1.0 src/decode-codepoint.ts.
The entities C1/replacement logic carries its upstream adaptation pointer,
https://github.com/mathiasbynens/he/blob/36afe179392226cf1b6ccdb16ebbb7a5a844d93a/src/he.js#L106-L134.
The actual licenses follow.

## markdown-it notice

Copyright (c) 2014 Vitaly Puzrin, Alex Kocharin.

Permission is hereby granted, free of charge, to any person
obtaining a copy of this software and associated documentation
files (the "Software"), to deal in the Software without
restriction, including without limitation the rights to use,
copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the
Software is furnished to do so, subject to the following
conditions:

The above copyright notice and this permission notice shall be
included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES
OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT
HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY,
WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
OTHER DEALINGS IN THE SOFTWARE.


## entities notice

Copyright (c) Felix Böhm
All rights reserved.

Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met:

Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer.

Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution.

THIS IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF THIS,
EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.


## he adaptation notice at revision 36afe179

Copyright Mathias Bynens <https://mathiasbynens.be/>

Permission is hereby granted, free of charge, to any person obtaining
a copy of this software and associated documentation files (the
"Software"), to deal in the Software without restriction, including
without limitation the rights to use, copy, modify, merge, publish,
distribute, sublicense, and/or sell copies of the Software, and to
permit persons to whom the Software is furnished to do so, subject to
the following conditions:

The above copyright notice and this permission notice shall be
included in all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND
NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE
LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION
OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION
WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

