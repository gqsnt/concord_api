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

**Correction (found by ADV-6 below):** the C4-R-005 code change did not land in `24717e8`. The
scripted replacement in `prepared.rs` silently matched nothing (the file had been reformatted),
and `c4_r_005_unit_drains_large_body` passed anyway because it did not discriminate buffering from
draining. Re-fixed and re-tested with a discriminating test; scripted edits now assert their match
count.

## Claude adversarial review — `claude/adversarial_24717e8.md` (Opus 5.5 XHigh subagent, SHA 24717e8)

Independent of the Codex findings (the reviewer was told not to read them). All findings came with
reproductions in the reviewer's scratch crate.

| ID | Sev | Disposition | Evidence / fix | Regression test |
|---|---|---|---|---|
| ADV-1 | medium | CONFIRMED | A TLS name-mismatch error deep in reqwest's `source()` chain contained a `#[sensitive]` host label (reproduced by the reviewer with `openssl s_server`). Fix: for endpoints with a sensitive label, `Error::source()` returns `None`; `Error::transport()` stays as an explicit, documented-unredacted accessor. | `sensitivity.rs::adv_1_sensitive_host_hides_transport_source` |
| ADV-2 | medium | CONFIRMED | Safety-list sensitivity reached generated `Debug` but not path segments, labels or other keys using the same value (`path "keys/{client.token}"` printed the token). Fix: sensitivity is computed to a fixpoint — a value withheld anywhere makes every segment, label and key that reads it sensitive. | `sensitivity.rs::adv_2_effective_sensitivity_reaches_every_placement` |
| ADV-3 | medium-low | CONFIRMED | Generated locals (`__p`, `__v`, `__c`) carried user-token spans, so `api!` forwarded through `macro_rules!` failed with hygiene errors. Fix: only the user value expression is respanned. | `tests/hygiene.rs` |
| ADV-4 | low-medium | CONFIRMED | Expansions hard-coded `::concord::`, breaking a renamed dependency (needed for V3/V4 side by side). Fix: `concord::api!` is a `macro_rules!` forwarding `$crate` to the proc macro. | `examples/shapes` renames nothing; `tests/renamed` downstream crate |
| ADV-5 | low | CONFIRMED | `base "http://api.0x10"` passed the macro's subset check but `url` parses a hex last label as IPv4 → constructor panic. Fix: a last label that is numeric or `0x`-hex is rejected unless the whole host is a dotted-quad IPv4. | `ui/base_invalid_port.rs` |
| ADV-6 | low-medium | CONFIRMED | `Decode::DISCARD` was never read (see correction above). | `bodies.rs::adv_6_discard_decoders_receive_no_bytes` |
| ADV-7 | low | CONFIRMED | An invalid `Encode::CONTENT_TYPE` panicked in `HeaderValue::from_static`. Fix: validated, invalid → `ErrorKind::Build`. | `bodies.rs::adv_7_invalid_content_type_is_a_build_error` |
| ADV-8 | low | CONFIRMED | A literal header value that can never be valid compiled and failed on every call. Fix: header template literals must be visible ASCII, space or tab at compile time. | `ui/header_literal_value.rs` |
| ADV-9 | low | CONFIRMED | `{r#self}` (and `r#super`, `r#crate`, `r#Self`, `r#_`) panicked the macro. Fix: rejected with a spanned error. | `ui/template_raw_keyword.rs` |
| ADV-10 | low | DESIGN_TRADEOFF (documented) | `each` iterates any `IntoIterator`; on `Option<T>` that means 0 or 1 occurrences, and `override "k" = each opt` with `None` clears `k`. Explicit by the use of `each`; rejecting it would need type-name inspection. Documented in SEMANTICS §5. | — |
| ADV-11 | info | CONFIRMED | clippy `too_many_arguments` fired on generated entry methods with many required values, and users cannot silence it inside `api!`. Fix: generated entry methods carry `#[allow(clippy::too_many_arguments)]`. | `tests/hygiene.rs` (9 required values under `-D clippy::all` in GATES) |

Gap noted, not changed: a secret inside a runtime base URL (webhook-style path) is shown by client and
`Prepared` `Debug`, because the base is not a declared value (SEMANTICS §9 lists it as outside the
guarantee).

Side effect of the ADV-4 fix (`DESIGN_TRADEOFF`): because `concord::api!` is now a `macro_rules!`
wrapper, two rustc trait errors (a scalar placement of an `Option<T>`/`Vec<T>` value, which has no
`Display`) are reported at the `api!` invocation instead of at the declared type; the message still
names the type (`ui/option_has_no_wire_spelling.stderr`, `ui/vec_needs_each.stderr`). All
Concord-generated diagnostics keep their exact spans.
