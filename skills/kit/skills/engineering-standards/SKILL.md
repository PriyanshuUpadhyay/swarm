---
name: engineering-standards
description: Supply scoped engineering rules before design or coding and during review of API contracts, data models, queries, retries, deadlines, quotas, and entitlement failures. Distinguish protocol requirements, project rules, and advice. The calling workflow owns code changes and review verdicts.
---

# Engineering standards

Select rules from the requested behavior and existing repository contracts, or say no rule covers
it. Do not ask the user to repeat engineering standards or invent product requirements. The
calling workflow keeps its edit authority and approval steps; this skill supplies rules.

## How to read a row

A row earns its place by stating a principle that stays true across engines and products. An
engine or product name may appear in `Scope` or `Check` to say where a check runs, as SQLite does
for D1. It may not appear in the rule text as the reason the rule holds. An incident belongs in
`Source`, as evidence. A rule that is true only inside one product belongs in that product's
repository instead of here.

`Status` — **MUST** is a protocol requirement. **PROJECT** is a rule this repository adopted.
**ADVICE** is vendor or community guidance. MUST beats PROJECT beats ADVICE. A citation alone does
not prove a rule applies; apply a row only inside its `Scope`.

`Check` — a command that is available, or `judgment`. Use the row's expected output and the tool's
exit status together. A missing command or execution error is **incomplete**, never a pass.
`PRAGMA foreign_key_check` can print violations and still exit 0; other tools can print data on a
pass. Neither exit 0 nor nonempty output alone decides the result. Confirm a check against a
known-bad case before trusting it; a green tool result does not prove a `judgment` row.

## Procedure

1. Derive the intended behavior, failure cases, and applicable row IDs from the request and
   repository. Keep them in the existing plan or task record, not a second document.
2. Read the sections that contain those rows and check their scope.
3. Run each `Check` that fits the actual engine. Report what you could not run.
4. When a `Scope` forks, two rows conflict, or the question is contested, stop and emit the
   section 5 block. Do not rule.
5. End with `pass N / fail N / judgment M / not checked K`. A fail quotes the row
   and the output that failed it.

## 1. API contract

| # | Rule | Status | Scope | Check | Source |
|---|---|---|---|---|---|
| A1 | JSON has no `Infinity` or `NaN` token. A quoted `"INFINITY"` is valid JSON but breaks a numeric field's type. | MUST | JSON payloads | judgment | RFC 8259 §6 |
| A2 | A 401 from a bearer-protected resource must include a `WWW-Authenticate: Bearer` challenge. Use `error="invalid_token"` for an expired or invalid token. With no authentication information, normally omit the error code; `realm` is optional. A response body does not replace the challenge. | MUST | bearer-protected resource endpoints; preserve RFC SHOULD/MAY strength within the rule | judgment | [RFC 6750 §§3–3.1](https://datatracker.ietf.org/doc/html/rfc6750#section-3.1) |
| A3 | Do not return a field whose value is computable from other fields in the same payload. | ADVICE | response bodies | judgment | Azure `rest-no-computable-fields` |
| A4 | Treat an enum as extensible and document that new values may appear, unless the set can never change. | ADVICE | response enums | judgment | Azure `json-use-extensible-enums` |
| A5 | `null` and an absent property must mean the same thing. One `null` cannot carry three meanings. | ADVICE | request and response bodies | judgment | Zalando 123 |
| A6 | A boolean must not be null. If a third state is meaningful, use a named enum instead. | ADVICE | request and response bodies | judgment | Zalando 122 |
| A7 | A quantity with a unit carries the unit as a field-name suffix. | ADVICE | numeric fields | judgment | AIP-141 |
| A8 | A timestamp field is not named in the past tense, and an integer timestamp names its unit. `reset_time_millis`, not `resets_at`. | ADVICE | time fields | judgment | AIP-142 |
| A9 | Model a variant as its own shape under `oneOf` with a `discriminator`, rather than as a sentinel inside a typed field. This is the answer to "should unlimited be -1 or null". | ADVICE | payloads with variant states | `vacuum lint` or `spectral lint` if installed | OpenAPI 3.1 |
| J2 | State the mapping between a stored column and its wire field when a value carries a special meaning. The two need not share a representation, and an undocumented mapping is the defect. | PROJECT | a persisted value with a sentinel or variant | judgment | local incident, `GET /usage` |
| A11 | Serialize declared 64-bit integer fields as decimal strings, including values inside the safe integer range. Parsers may accept numbers; that does not change the serializer's output rule. | MUST | canonical ProtoJSON output for declared 64-bit integer fields, not arbitrary JSON numbers | judgment; compare output with the protobuf field type; for an int64 field, `{"id": 1}` is a bad output and `{"id": "1"}` is the expected output | [ProtoJSON type mapping](https://protobuf.dev/programming-guides/json/#representation-of-each-type) |
| A12 | For exact integers outside the range -(2^53-1) to 2^53-1, agree on an encoding that the actual consumers can read without loss. A decimal string is one option; preserve the declared API contract. | ADVICE | general JSON APIs whose consumers may use binary64 numbers; no universal string requirement | judgment; check the field schema and round-trip boundary values through the actual consumers | [RFC 8259 §6](https://www.rfc-editor.org/rfc/rfc8259#section-6) |

## 2. Data model and schema

| # | Rule | Status | Scope | Check | Source |
|---|---|---|---|---|---|
| D1 | Every foreign key has a covering index. | ADVICE | SQLite | `sqlite3 db '.lint fkey-indexes'` | SQLite CLI |
| D2 | Declared foreign keys must resolve. | MUST | SQLite | `sqlite3 db 'PRAGMA foreign_key_check'` | SQLite CLI |
| D3 | Name the storage topology before choosing a key. A time-ordered key keeps inserts near the tail on one node, and the same key hotspots one shard when the store is sharded. | ADVICE | **forks: single-node vs sharded** | judgment | RFC 9562; Spanner guidance |

Contested, so it gets no row. A `deleted` boolean does not scale, and neither does a large
`DELETE`, yet a hard delete with an audit table breaks its own trade-off. Send it to section 5.

A clean linter run does not prove normal form. Normalisation stays `judgment`.

## 3. Performance and efficiency

| # | Rule | Status | Scope | Check | Source |
|---|---|---|---|---|---|
| P1 | Quote a percentile, never a mean. A mean hides the tail, and a load generator that pauses during a stall under-reports it (coordinated omission). | ADVICE | any latency claim | judgment | Dean and Barroso, *The Tail at Scale* |
| P2 | Judge a scan by table size, rows needed, and measured cost against the workload's budget. A scan of a small table or most of its rows can be appropriate. Investigate costly scans before adding an index. | ADVICE | SQLite queries on hot paths with a known workload | judgment; inspect `EXPLAIN QUERY PLAN` and measure representative data; `SCAN` alone is not a failure | [SQLite query planning](https://www.sqlite.org/queryplanner.html) |

A plan shows shape, never production cost, so P2 passing is not a benchmark.

## 4. Reliability and failure policy

| # | Rule | Status | Scope | Check | Source |
|---|---|---|---|---|---|
| R1 | Derive every outbound timeout from the caller's remaining deadline, and propagate it. A call with no deadline is an unbounded queue, and a per-call timeout still lets a chain outlive its caller's budget. | ADVICE | any network call, and any chain of two or more hops | judgment | AWS Builders' Library; gRPC service config |
| R2 | A retried mutating request carries an idempotency key, and the server stores the result against it. | ADVICE | mutating endpoints | judgment | draft-ietf-httpapi-idempotency-key-header |
| R3 | Cap retries with a budget, and make the backoff exponential and jittered. A per-call retry count amplifies a correlated failure, where one study measured 41.5% success with retries against 55.4% without, and fixed backoff synchronises callers into a herd. | ADVICE | any retry | judgment | AWS, Exponential Backoff And Jitter; gRPC `retryThrottling`; arXiv 2608.25403 |
| R4 | Fail closed when an authorization read fails, and say so. A failed read that yields "no plan" is indistinguishable from a real free user. | MUST | any authorization or entitlement read | judgment | local incident, `GET /usage` |
| J1 | Test and reserve against a limit in one statement, e.g. `UPDATE ... SET used = used + 1 WHERE used < limit`. A separate check then write lets two callers both pass, and a tally written after the work reserves nothing. | PROJECT | a counter or tally compared against a limit | judgment | local incident, `GET /usage` |

Whether retry-with-backoff or backpressure and load-shedding should be the default primitive is
**contested**. It gets no row. Send it to section 5.

## 5. Framing a trade-off

This section has no table, because no rule and no checker decides a trade-off. Use it when a `Scope`
forks, two rows conflict, or an item is contested. Emit this block and stop.

```text
Fork:            <the choice, in one line>
Options:         <two or three, named>
Cost of each:    <what each one buys and what it spends>
Who is hurt:     <which caller, user, or operator feels it>
Reversibility:   <cheap to undo, or one-way>
What would settle it: <the measurement or fact that decides it>
Recommendation:  <one option, with the reason>
Owner:           human
```

It recommends and never rules. Emit it only for a trade-off that is genuinely open, not for a branch
that a known fact already settles.

To record the decision once made, use `decisions`. This skill does not write an ADR.

## Refusals

- No `APPROVE` or `REQUEST CHANGES` verdict, and no coverage claim. `review-check` owns those.
- No ADR, no decision record. `decisions` owns those.
- No security rules. Out of scope by the user's instruction.
- This skill supplies no implementation workflow, migrations, or installs; the caller owns them.
- No answer outside these tables. Say "no applicable rule in this skill" instead of improvising.
- No rule without a citation, a runnable check, or a named local incident.
