# Review finding triage

Dispositions: `CONFIRMED` (reproduced or proven from source; fixed or scheduled), `REJECTED_WITH_EVIDENCE`,
`DESIGN_TRADEOFF`, `OUT_OF_SCOPE`, `OPEN`. Raw reports are kept unedited next to this file.

## Codex Gate 2 — `codex/gate2_20261101T163326Z_2639b39.md` (GPT-6.1 Sol XHigh, read-only, SHA 2639b39)

The reviewer could not build or bind loopback sockets inside its read-only sandbox; its evidence is
source reading plus rustc MIR experiments against cached artifacts. Each finding was re-checked by the
coordinator against source and, where marked, reproduced by a new test.

| ID | Sev | Disposition | Evidence / fix | Regression test |
|---|---|---|---|---|
| C4-R-001 | high | CONFIRMED | Handle/endpoint `Debug` redacted only `#[sensitive]` values; a value placed under a safety-list name (`Authorization`) or into a key made sensitive by an ancestor was printed. Fix: the macro computes *effective* sensitivity per value — marked, or contributing to any slot that is sensitive on any chain — and all generated `Debug` uses it. | `sensitivity.rs::c4_r_001_debug_uses_effective_sensitivity` |
| C4-R-002 | medium | CONFIRMED | Hole-free quoted values were emitted with format-string escaping (`{{ok}}` sent literally). Fix: hole-free templates emit the decoded literal. | `composition.rs::c4_r_002_escaped_braces` |
| C4-R-003 | medium | CONFIRMED | A qualifier equal to the *current* endpoint's name was rejected before ancestors were searched (`scope repo { GET repo() { path "{repo.x}"; } }`). Fix: ancestors are searched first; the "current owner" diagnostic applies only when no ancestor matches. | `composition.rs::c4_r_003_endpoint_named_like_scope` |
| C4-R-004 | medium | CONFIRMED | The macro's base validation was a partial second parser; an invalid port reached a panicking `Base::from_static`. Fix: the macro accepts only a strict, explicitly specified subset (scheme, LDH/IPv4/bracketed-IPv6 host, numeric port ≤ 65535, `pchar` path) that `url` always accepts; anything else is a declaration error. | `tests/ui/base_invalid_port.rs`, `base_errors.rs`, `base_forms` unit test |
| C4-R-005 | medium | CONFIRMED | `()` buffered the whole body before discarding it. Fix: `Decode::DISCARD` (default `false`); `()` sets it and the decoder drains chunk by chunk. | `bodies.rs::c4_r_005_unit_drains_large_body` |
| C4-R-006 | medium | CONFIRMED | `--no-default-features` could not build the test suite (`Json` used unconditionally). Fix: JSON fixtures declare `required-features = ["json"]`; all other suites run in every feature combination. | feature matrix in `evidence/GATES.md` |
| C4-R-007 | medium | CONFIRMED | Let chains (Rust 1.88) under a declared `rust-version = "1.85"`. Fix: let chains removed; the declared minimum is built and tested with an installed 1.85 toolchain (see `evidence/GATES.md`). | MSRV run |

Notes from the same report adopted as documentation (no code defect): non-ASCII header values are
rejected by policy (SEMANTICS §5); `remove` removes Concord contributions only — a reqwest client
default header can still fill the name (SEMANTICS §5, tested by
`composition.rs::client_default_headers_fill_only_missing_names`); the base path is the canonical
path produced by URL parsing (SEMANTICS §4); a required body is a positional argument **in
declaration order** (changed: it was always last); the loopback fixture does not decode chunked
request bodies, so streaming request bodies are not covered (TEST_MATRIX).

Codex's opinion on the contested D4 point: retain D4 for this iteration, conditional on the
discriminating tests (absent override keeps, present-empty `each` clears, nested overrides pick the
innermost present value). Those tests exist (`q4`, `q5`, `q10`, `remove_then_introduce_absent_removes`).
Disposition: `DESIGN_TRADEOFF`, kept.
