# transcript-patch checks

This directory holds the regression checks for the private Rust patch core in `examples/transcript_patch_trial.rs`
(`src/view/transcript/display_patch.rs`, `display_patch/kinds.rs`, `display_comments.rs`).

The checks compare the trial binary with frozen values from the ORIGINAL JavaScript (git `c8ca837e1b70cbb4479bfda387f3bace2931749e`,
run on Node 22.23.2 with `--experimental-transform-types`). The frozen values cover only these functions:

- `transcript.ts`: `parsePatch`, `patchKinds`, `isPatch`, `splitPatch`.
- `changes.ts`: `diffLines`, `commentRange`.

No expected value comes from Rust. Other ORIGINAL functions, for example `wordAt` and `renderMarkdown`, are not covered here.

| File | Role |
| --- | --- |
| `patch_check.py` | The runner. Its code owns every rule, verdict and exit code. |
| `patch_inputs.json` | Frozen inputs, groups `legit`, `over` and `inf`. |
| `patch_expected.json` | Frozen ORIGINAL values. |
| `make_expected.py` | Generator of the two data files. It reads a sealed local capture, so it cannot run on a fresh checkout. |
| `FREEZE.sha256` | Hashes of the five files above. The runner checks them before it starts any process. |

## Commands (run from the repository root)

Build the trial binary. A plain `cargo build` does not build examples.

    cargo build --locked --offline --example transcript_patch_trial

Run the observation. `--root` must be a NEW path that you own. The runner creates it and refuses an existing one.

    python3.12 -B -I trials/transcript-patch/checks/patch_check.py \
      --exe "$PWD/target/debug/examples/transcript_patch_trial" \
      --root "$OWNED_NEW_DIR" --owner "<who runs this>" [--runs 2] [--expect-exe-sha256 HEX]

Run the Rust guard test. This command is REQUIRED. A plain `cargo test` does not run the tests inside an example.

    cargo test --locked --offline --example transcript_patch_trial

Optional style checks:

    cargo fmt --check
    cargo clippy --offline --locked --all-targets -- -D warnings
    cargo clippy --offline --locked --features serve --all-targets -- -D warnings

The runner states its own rules, verdicts and exit codes in its code and its messages. To see the runner self-check, run
`python3.12 -B -I trials/transcript-patch/checks/patch_check.py --selftest`.

## Limits

- The runner covers valid UTF-8 text only and the patch and comment rows. Other task outputs are only checked for their fixed layout.
- The data files hold ORIGINAL JavaScript values with numbers as binary64. A native `Infinity` is JSON `null`.
- `make_expected.py` needs the sealed local capture it names. To prove a value again, run the ORIGINAL JavaScript at `c8ca837e`.
- The contract is `PATCH-V1.md` and the audit record is `RESULT.md`. They are local run records, not installed files. They can be found
  only in `tmp/flow/2026-11-04-swarm-audit-the-transcript-translater/unit09/{PATCH-V1,RESULT}.md` on the machine that ran the audit.

## History

The earlier runners `checks` and `checks-v2` to `checks-v7` stay unchanged in that run folder as audit evidence. Keeping them is not a
proof that a problem was removed and not an approval of it. Nothing in the run folder was cleaned, and what was not checked stays unknown.

- `checks-v2` is UNSAFE. It sent a kill signal to a process group three times after the group leader was reaped. Later versions
  stopped this, but no later proof makes that past violation clean.
- The round-6 and round-7 observer controls used `sleep` calls and long stalls. They break the repository test-timing rule (ADR 0134).
- Separately, the reference retrieval was an auto-backgrounded Cargo fetch, and it made extra Git reads. This is a different
  matter from the controls above.
