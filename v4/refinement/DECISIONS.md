# Refinement decisions (D01–D18)

Decision board: handoff `scenarios/DECISION_BOARD.md`. The alpha (`8f7adc2`) was the control in
every comparison. Evidence: independent specialist tracks (Claude Opus 5.5, xhigh) in
`evidence/tracks/` (`calling.md` = B, `authoring.md` = A, `core.md` = C, `compiler.md` = D),
a fresh-use trial of the control (`evidence/tracks/fresh_control.md`), and the Codex
(`gpt-6.1-sol`, xhigh) design challenge — a blind pass written from the charter and task corpus
only, then a source-comparison pass on a snapshot of `8f7adc2` (`reviews/codex/design_pass*`).
Agent trials are simulations, not human studies.

| ID | Question | Disposition | Commit |
|---|---|---|---|
| D01 | Root construction | **CHANGE**: `Client::new(http, ClientConfig { .. })`; scopes/endpoints stay positional | `2720d67` |
| D02 | Optional control vocabulary | **KEEP** `set_x` / `maybe_x`; every value now has `set_x`; docs fixed | `2720d67` |
| D03 | Default lifecycle / reset | **KEEP** (eager, once per owner construction; no reset) | — |
| D04 | Inherited optional override | **CHANGE**: `override "k" = v else inherit` required when `v` can be absent; `remove` acts first | `0864d38` |
| D05 | Query/header ordering | **KEEP** query slot order; **CHANGE** contract: no cross-name header order (L2) | `4f3ff85` |
| D06 | Representation syntax/defaults | **CHANGE**: `-> T via R`, `body b: T via R`; **REJECT** scoped defaults | `f29e9ba` |
| D07 | Wire formatting | **KEEP** `Display` + newtypes; **DEFER** a per-placement adapter; **REJECT** a wire-value trait | — |
| D08 | Type bounds / ownership | **CHANGE**: endpoint `Clone` exactly when its data is | `c2871f9` |
| D09 | Scope storage/snapshots | **KEEP** copy-on-write `Arc` handles; **CHANGE** wording: structural, not deep | `2720d67` |
| D10 | Non-wire client state | **KEEP** liveness rule; route = application wrapper; **DEFER** getters / `local` values | — |
| D11 | Base validation | **KEEP** strict static subset + maintained consistency test | `f0e203b` |
| D12 | Header construction | **CHANGE**: names validated natively at compile time; full map is a build error; failing `Display` is a build error | `650a174`, `ab0278c`, `b4ce491` |
| D13 | Native/typed response access | **KEEP** parts + decoder; **CHANGE** small: `Decoder::finish`, `into_response -> Result`, `BodyError: Error`; **REJECT** `request_mut`, an error-response framework | `f29e9ba`, `433abf9` |
| D14 | Attributes / naming | **CHANGE**: `#[cfg]`, `#[deprecated]` on scopes/endpoints; others still rejected | `0864d38` |
| D15 | Error/sensitivity surfaces | **KEEP** safe `Display`, no `source()`, explicit raw accessors; add positive controls | M4 |
| D16 | Compiler structure | **CHANGE** (small): explicit crate path instead of a thread-local | `8628622` |
| D17 | Codegen / inline | **KEEP** per-endpoint specialization and blanket `#[inline]` pending the M5 measurement | M5 |
| D18 | Documentation hierarchy | **CHANGE**: task-first README/guides, generated rustdoc facts | M6 |

## D01 — root construction (CHANGE)

**Problem.** `Client::new(http, a, b, …)` took required values positionally through
`impl IntoValue<T>` parameters. Track B's counterexample: an Algolia-shaped client
`Search(app_id: String, #[sensitive] api_key: String)` with `subdomain "{app_id}-dsn"` compiles
when the two are swapped; the secret becomes a DNS label and is printed by Concord's own `Debug`.
At 8 inputs a missing middle argument produced three errors blaming the wrong inputs (the
generic parameters defeat rustc's argument alignment). Credential pairs of one type are the
normal root shape for many providers.

**Alternatives** (B, all wire-equivalent): config struct, ordered steps, typestate builder, runtime
builder. Codex pass 1 proposed a named record for ≥ 1 input; pass 2 suggested ≥ 2.

**Choice.** A generated `{Client}Config` struct with one documented `pub` field per required
client value and a redacting `Debug`, whenever the client has a required value. One `E0063`
names every missing field; declaration reordering cannot swap call sites; the value docs render
(they were silently dropped). Scope/endpoint entries stay positional (arity 0–2 in practice,
reading in URL order). A threshold (positional at one value) was rejected: two shapes, and adding
a value flips the API.

**Accepted loss.** String literals need `.into()`; a one-token root is longer
(`GitHub::new(http, GitHubConfig { token })`). Same-typed fields can still be assigned crosswise
explicitly — newtypes remain the remedy for that.

**Also (fresh-use friction).** Required values had no setter, so rotating a supplied key meant
rebuilding the client and silently losing `with_base_url`. Every value now has `set_x`.

## D02 — optional vocabulary (KEEP)

Bare `x(v)` collides with navigation and built-ins (`E0592` for `region`, `send`) and takes the
getter name; `set_x` + `clear_x()` turns passing an existing `Option` (cursor pagination) into a
`match`. The real problem was documentation: `maybe_x(None)` on an initialized value means
"absent", not "default". Generated docs now state each value's starting state (the initializer's
text and when it runs) and `maybe_x`'s exact meaning.

## D03 — defaults and reset (KEEP)

Counters (B): one evaluation per owner construction, even when immediately overridden; none on
clone, prepare or send. A re-evaluating `reset` yields a *different* value; lazy evaluation gives a
cloned retry a different idempotency key. Codex withdrew its pass-1 "retained baseline reset"
after inspecting the source. Unsupported: restoring the originally evaluated value (use the
original handle).

## D04 — inherited optional override (CHANGE)

The fresh-use participant read `override` as "always replace" and never found the exact
replacement idiom; Track A's prediction study showed the same. **Alt-X** (plain `override` exact,
`else inherit` for fallback) was rejected: an author who forgets `else inherit` silently drops
the inherited default on the common path, and migration would silently change behaviour.
**Chosen (Alt-Y):** semantics unchanged, but an optional source must say `else inherit` (a
compile error names both choices; `else inherit` on a total source is an error). `remove` now
acts first within an owner, so exact replacement works with a direct placement (DEF-2) and a
same-owner introduce+remove is rejected (DEF-5); the post-`remove` diagnostic names the removing
owner (DEF-3). Cost: two tokens per optional override. Unsupported: a caller-side "remove the
inherited value" control — use `k?: Option<T>` with `each … else inherit`.

## D05 — ordering (KEEP; contract corrected)

Query: exact slot order. Headers: value order within one name; nothing across names (the native
`HeaderMap` does not promise it). Interleaved repeated query keys stay native: ~10 lines with
`into_parts` + `url_mut` + `Decoder::finish` (Track A, loopback-verified).

## D06 — representations (CHANGE)

Every generated signature was an associated-type projection
(`Result<<Json<Item> as Decode>::Output, Error>`, `impl IntoValue<<Json<NewItem> as Encode>::Value>`).
Now `-> Item via Json` renders `send(self) -> Result<Item, Error>`; custom formats are a marker
plus `Encode<T>`/`Decode<T>` impls (no `PhantomData`, no associated types); `Prepared<T>` and
`Decoder<T>` are generic over the payload only. A scoped default was rejected with executable
traps: under a JSON default `-> String` silently decodes JSON and a 204 `DELETE -> ()` becomes a
decode error. Accepted loss: `String via Text` / `Bytes via Binary` are slightly longer; the
public `DISCARD` hook is gone (an endpoint without `->` drains).

## D07 — wire formatting (KEEP / DEFER / REJECT)

`Display` + newtypes handles foreign types (Codex: one foreign IPv4 with three spellings through
wrappers). A per-placement adapter reaches the same wire with fewer lines but needs syntax in three
places and does not prevent the silent human-`Display` case. Revisit trigger: a concrete API task
where wrappers dominate the declaration.

## D08 — `Clone` follows the data (CHANGE)

A bodyless endpoint derived `Clone` (rejecting move-only values); a body endpoint was never
`Clone`. Now `impl Clone where for<'__concord> T: Clone, …` per value and body type — accepted on
1.85.1 and 1.97.1 (a plain bound on a concrete type is a trivially-false-bound error). The impl
simply does not apply when a value is not `Clone`; `.clone()` reports the unmet bound at the
declared type. Risk recorded: relies on rustc not checking global bounds under a binder
(rust-lang/rust#48214 moves toward accepting trivial bounds).

## D09/D10 — snapshots and non-wire state (KEEP)

Allocation counts (B): 1 per scope entry, 0 per endpoint call; a setter on a shared handle copies
that owner's values once. The docs claimed "requests already created keep their values", false for
interior-mutable values: wording now says *structural* snapshot. Non-wire state: a ~25-line
wrapper struct is the route; getters would collide like D02.

## D11 — base validation (KEEP)

No input accepted statically and rejected at runtime (C: 98-input corpus + 300k random). Option
"url in the proc macro" grows its dependency tree from 5 to 34 crates; a fallible constructor puts
`?` in every caller. The invariant is now a maintained test (corpus + 50k structured random
inputs against the real `Base::parse`). Unsupported statically: IDN hosts and escaped base paths
(use `with_base_url`).

## D12 — native header and formatting boundaries (CHANGE)

L1 (name > 65,535 bytes panicked in `prepare`): the native constructor runs in `const`; a full
`HeaderMap` (24,576 names) panicked in `append`/`insert`: now `try_*` and a build error; a failing
`Display` truncated segments/labels silently and panicked elsewhere: now a build error in every
placement. Remaining native boundary: hyper-util adding `host` to a full map panics at send time —
reachable only by a declaration with ~24,575 header names (compile memory ≥ 16 GB, C).

## D13 — native and typed response access (KEEP + small CHANGE)

All of U14–U16, U28, U29 work with parts + decoder. Added: `Decoder::finish(Result<Response,
reqwest::Error>)` (Codex: native execution lost Concord's protected transport conversion);
`Error::into_response() -> Result<Response, Error>` (it dropped non-status errors);
`BodyError: Display + Error` without `source()`. Rejected: `Prepared::request_mut` (C reproduced a
path push that makes the index-based redaction print `…/S3CR3T/<redacted>`) and a typed
error-document framework.

## D14 — attributes (CHANGE)

`#[cfg]` (whole subtree + navigation method) and `#[deprecated]` (navigation method) on scopes and
endpoints; everything else still rejected. Limitation: two cfg-exclusive endpoints cannot share a
name (names are checked as if every cfg were enabled).

## D16 — compiler structure (CHANGE, small)

The crate path is an explicit parameter (byte-identical expansion of 15,385 lines). Deferred, no
behaviour change: the dead initial sensitivity pass and composing twice per owner (D). Rejected:
splitting crates or a runtime interpreter.

## D17 — codegen and inline (KEEP pending M5)

Generated size is linear in endpoints, depth (leaf endpoints), inherited contributions and
override chains; quadratic only for endpoints at every level of a deep chain (inherent). Limits
documented: depth ~48 (E0320), 1,000 nested overrides crash rustc. `prepare` is duplicated per
consuming crate under `#[inline]`; dropping it from `prepare` alone is the candidate measured in M5.
