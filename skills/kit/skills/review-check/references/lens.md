# Lens

Files: `**/*.{ts,tsx,js,jsx,mjs,cjs,py,go,swift,m,rs,java,kt,rb,sql,sh}`

The review reads both parts for every unit, in every language. Each line is "what to flag → what
to ask for". The repo rules file and the domain catalog's `review` paths add to this list and win
when they disagree. `engineering-standards` owns the rows for API shapes, keys and indexes, query
cost, retries, and timeouts, so cite its row id instead of restating it.

Example. A diff adds `function parseId(v: unknown): string` that trims and checks a value that
the same service wrote one call earlier. The cleanup part flags "validation of data we produced",
and the comment asks if the helper is needed, because our own code sets the value.

## Cleanup

The goal is a change that the next reader can follow without holding extra state in their head.

- `C-1` A helper, wrapper, or file with one caller and no domain name → inline it. `minimize-reader-load` owns the conditions.
- `C-2` A shallow module whose interface is as big as its body → merge it into the caller (Ousterhout, deep modules).
- `C-3` Validation, `unknown`, or a zod schema on data that our own code produced → type it and drop the check. Keep checks only at a trust boundary: user input, LLM output, a third-party API.
- `C-4` A second type for a shape that already has one (a hand-written RPC interface next to the real class, a type that repeats a schema) → derive it from the owner.
- `C-5` The same logic shape in two hunks → one shared piece, or say which one stays (Fowler, duplicated code).
- `C-6` A boolean flag that adds a mode next to an older flag → one enum.
- `C-7` A magic number or repeated literal → a named const next to its use.
- `C-8` A name that does not say what the value holds → rename. A name that needs a comment to make sense → rename first, comment second.
- `C-9` A non-obvious rule, invariant, or workaround with no comment → a one-line comment. A comment that restates the code → remove it.
- `C-10` Debug routes, repair scripts, or feature flags that the change no longer needs → remove them.
- `C-11` Speculative parameters, hooks, or config with no current caller → remove them (Fowler, speculative generality).
- `C-12` One change that forces edits in many unrelated files → ask where the concept should live (Fowler, shotgun surgery).

## Logic

The goal is a change that stays correct under real load, real failures, and real data.

- `L-1` A storage, RPC, or network call inside a loop → one batched call. Name the loop bound.
- `L-2` A list or query with no limit, or a limit with no signal to the caller → a bounded page and a cursor, or a truncation flag the consumer can act on.
- `L-3` Two writes that must both happen, or neither → one transaction. A check, then a write, on a shared counter → one conditional statement (`engineering-standards` J1).
- `L-4` A read-modify-write that two requests can interleave → name the race and the serializing owner.
- `L-5` An error path that swallows the error, or turns "failed to read" into "empty" → fail loudly, or say why empty is safe (`engineering-standards` R4 for authorization reads).
- `L-6` An outbound call with no timeout, or a retry with no budget → cite `engineering-standards` R1 and R3.
- `L-7` A loop whose exit depends on data (`for (;;)`, a date search) → a proven bound or a max iteration count.
- `L-8` Memory that grows with user data (whole tables, whole files, full histories in one array) → stream, page, or cap it against the runtime limit.
- `L-9` Text cut by a byte or UTF-16 index before it goes to an LLM or a user → cut by code point or grapheme so a multi-byte character stays whole.
- `L-10` Time zones, day boundaries, and date math done by hand → the repo's date library.
- `L-11` A migration that rewrites or rebuilds existing data or indexes → an additive change, or proof that the rewrite is bounded.
- `L-12` A contract that a caller relies on (return shape, ordering, null meaning) that the change alters → name each caller that breaks. Search callers before you claim none break.
- `L-13` A client, stub, connection, or session resolved once before a retry loop (or cached in a field or closure that each retry reuses) → re-acquire it inside each attempt. A broken handle fails every retry at once, and the retry budget is spent in zero time. Applies: `(?i)(retr(y|ies)|attempt|backoff)`. Source: https://github.com/cloudflare/agents/issues/1918 (all step retries failed on one cached stub after a platform reset); https://developers.cloudflare.com/durable-objects/best-practices/error-handling/ (make a new stub after an exception)

## Language lenses

Each `lens-<name>.md` file next to this one adds rules for one language or platform. The script
picks the rules for a unit by each file's and each rule's `Files:` and `Applies:` lines. A missing
language lens never skips a unit.

Sources: John Ousterhout, *A Philosophy of Software Design* (deep modules, information hiding,
cognitive load); Martin Fowler, *Refactoring* ch. 3 (code smells); Google Engineering Practices,
"What to look for in a code review" (design, functionality, complexity, naming, comments).
