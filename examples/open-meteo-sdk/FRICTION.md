# SDK author observations

Observed on 2026-11-06 against frozen Concord `0983a349`, Rust 1.99.1.
Paths below are relative to `examples/open-meteo-sdk/`. Ordered by the authored
path from declaration through caller and failure handling. These are observed
actions, diagnostics, interventions and results; no library changes were made.

## F1 — Request composition and operation metadata worked directly

**Cause/category:** Concord, positive observation.

`src/lib.rs:23` declares the only forecast operation. Concord supplies the
encoded path/query/header composition and execution. `src/lib.rs:96` receives
it once and captures the real head, preserving generated identity. No manual
URL assembly, private reach-through, fabricated status or hand-authored
operation identity was needed in production code.

**Reproduction/result:** `tests/protocol.rs:46` asserts the exact prefixed target,
encoded timezone, method, headers and zero body bytes. `tests/protocol.rs:103`
checks a comma-separated selection is one encoded query value.
`tests/workflows.rs:147` requests both formats through the same public client
and observes `ForecastApi.forecast` on both responses. These tests pass.

## F2 — Runtime representation needs an SDK response adapter

**Cause/category:** Concord API boundary plus SDK domain policy.

Concord's `response F` and output type are static; `Decode::context` receives
the response head, not the requested format. This SDK must reject a JSON success
when CSV was requested, preserve original CSV bytes and return a typed runtime
enum. The declaration therefore uses public `-> stream` at `src/lib.rs:35`;
the public domain method at `src/lib.rs:73` binds query/Accept together, uses
`prepare().receive()`, checks media, bounds the body and selects the decoder
at `src/lib.rs:109`. This is a finite download adapter, not a network stream
offered to callers. It does not rebuild HTTP execution or expose native
reqwest responses.

**Reproduction/result:** `tests/workflows.rs:147` changes only the runtime enum;
`tests/protocol.rs:243` serves wrong or missing media with a stalled body and
confirms refusal before reading. Both pass. The adapter is working public-API
code, but it reconstructs response dispatch/status/error orchestration that a
single static typed `send()` would otherwise supply. No claim is made that
another spelling could not reduce this adapter.

## F3 — The CSV document contains two tables with different schemas

**Cause/category:** Provider envelope; SDK adapter work using Concord's raw route.

The pinned official CSV writer emits a location header/row before the hourly
header/rows, so a conventional `Csv<HasHeader>` table has incompatible widths.
`src/csv.rs:18` uses a flexible profile and `src/csv.rs:26` uses `NoHeader` to
keep both headers as records. `src/csv.rs:28` checks media as a document with
headers. `src/csv.rs:108` then recognizes the two provider tables, checks
their own widths/duplicate names and maps selected columns by variable name.
`src/csv.rs:184` preserves unselected columns with their full header text.
The parser, UTF-8 conversion, limits and quoting remain Concord's public API.

**Reproduction/result:** `tests/workflows.rs:65` serves the reordered,
wind-column perturbation in single-byte HTTP chunks. Exact bytes, metadata,
units, multiline quoted text and unknown fields survive. `tests/protocol.rs:286`
rejects header and width failures. Both pass. Reapplying table width/header
rules is envelope-specific work here, rather than a second generic parser.

## F4 — Unterminated quoted field was accepted at EOF

**Cause/category:** Concord CSV / underlying permissive generic parser behavior;
SDK completion-policy workaround.

The first negative run failed at `unwrap_err()`: the frozen decoder accepted
`fixtures/unterminated-quote.csv` as three valid forecast rows, including the
final value `12.7`, despite the unmatched opening quote. Concord's public raw
reader independently returns six records and `Completion::EndOfInput` for
those same bytes.

**Reproduction:** `tests/protocol.rs:437`,
`frozen_csv_decoder_quote_eof_reproduction_and_sdk_guard`, directly exercises
that public raw reader and then the SDK call. Run this named test with the
standard prefixed cargo-test command and `-- --exact`.

**Intervention/result:** `src/csv.rs:85` now checks that the body has an even
number of quote bytes before calling the parser. Doubled escape quotes preserve
parity. The fixture now returns `ResponseErrorKind::Csv` with an unmatched-quote
label; the direct decoder behavior remains visible in the passing regression
test. This extra generic completion check felt like reconstructing library
work and is deliberately retained in the SDK while Concord is frozen. It is a
targeted unmatched-quote guard, not a complete strict RFC 4180 validator.

## F5 — Metadata survives, but typed provider errors still require orchestration

**Cause/category:** SDK error policy; Concord's public response API made it possible.

At `src/lib.rs:98`, status is checked before the requested success format:
Open-Meteo sends JSON errors even for CSV requests. The adapter checks JSON
media and attempts a bounded error-document decode, preserving its failure as
`ErrorBodyError`. `src/error.rs:80` keeps the head across media/body/transport
failures and adds SDK accessors. Capturing metadata before consumption and
mapping it into every later error is manual adapter work; the head itself is
Concord's `ResponseMeta`, so no header/status structure was rebuilt.

**Reproduction/result:** `tests/protocol.rs:137` verifies typed 400 errors for
both representations; `tests/protocol.rs:171` preserves 429/retry metadata
after malformed JSON; `tests/protocol.rs:377` rejects incomplete HTTP framing
even after complete parseable CSV bytes. All pass. Non-JSON errors are refused
without consuming a stalled body at `tests/protocol.rs:193`.

## F6 — Provider specification needs additional CSV evidence

**Cause/category:** Provider docs/specification.

The requested OpenAPI file describes JSON but did not contain the CSV selector
or two-table CSV wire schema. The author checked the maintainer's accepted
`format=csv` answer and pinned official `CsvWriter.swift`, rather than claiming
the CSV fixture was an official downloaded example. Source pins, hashes,
access date and every transformation are in `PROVENANCE.md:7`.

**Reproduction/result:** inspect the pinned sources in that table and compare
`fixtures/forecast.csv:1` and `fixtures/forecast.csv:4` with the writer's location
and hourly headers. CSV fixture bytes are hand-authored and their notes/extra
columns are explicitly synthetic. No provider calls were made.

## F7 — Declaration diagnostics corrected initial SDK mistakes

**Cause/category:** SDK author mistakes, with useful Concord/rustc diagnostics.

The first compile used `header accept: String`; the macro reported that header
placement must name a field and supplied `header("x-api-key") key: String` as
the form. It also produced downstream type-inference errors after that
declaration failed. Replacing it with `header("accept") accept: String` at
`src/lib.rs:32` resolved those errors. The initial CSV limit calls supplied
`Some(u64)`; rustc pointed to the public setters expecting plain `u64`.
`src/csv.rs:21` now uses those signatures. No library workaround was needed.

## Verification and reading trace

The first locked test command refused the absent package lock entry. One
offline unlocked test invocation added only this package to `Cargo.lock`.
Subsequent locked tests passed: **16 protocol + 6 workflow + 2 compiled
documentation examples**, no ignored tests. Clippy with `--all-targets --
-D warnings` and package formatting pass. All test networking is loopback.

The root README, author/caller/custom-format guides, crate READMEs and public
rustdoc were consulted; `cargo doc -p concord --no-deps --features csv,jsonl`
completed with the required environment prefix. The Salesforce CSV adapter
and the Frankfurter metadata/error adapter supplied public-API patterns.
The SDK uses only `concord` facade types; there are no direct core/extras
dependencies, no private library references and no library edits. No
`tests/first_use.rs` was created; that remains the independent consumer's role.
