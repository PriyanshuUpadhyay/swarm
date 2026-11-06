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

