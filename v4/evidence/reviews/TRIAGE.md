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
| C4-R-004 | medium | CONFIRMED | The macro's base validation was a partial second parser; an invalid port reached a panicking `Base::from_static`. Fix: the macro accepts only a strict, explicitly specified subset (scheme, LDH/IPv4/bracketed-IPv6 host, numeric port ≤ 65535, `pchar` path) that `url` always accepts; anything else is a declaration error. | `tests/ui/base_invalid_port.rs`, `tests/ui/base_errors.rs`, `composition.rs::c4_r_004_strict_base_subset_constructs` |
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
| ADV-4 | low-medium | CONFIRMED | Expansions hard-coded `::concord::`, breaking a renamed dependency (needed for V3/V4 side by side). Fix: `concord::api!` is a `macro_rules!` forwarding `$crate` to the proc macro. | `examples/renamed` (depends on `concord` as `concord4`) |
| ADV-5 | low | CONFIRMED | `base "http://api.0x10"` passed the macro's subset check but `url` parses a hex last label as IPv4 → constructor panic. Fix: a last label that is numeric or `0x`-hex is rejected unless the whole host is a dotted-quad IPv4. | `ui/base_invalid_port.rs` |
| ADV-6 | low-medium | CONFIRMED | `Decode::DISCARD` was never read (see correction above). | `representations.rs::unit_and_discard_representations_drain` (moved from `bodies.rs`) |
| ADV-7 | low | CONFIRMED | An invalid `Encode::CONTENT_TYPE` panicked in `HeaderValue::from_static`. Fix: validated, invalid → `ErrorKind::Build`. | `representations.rs::invalid_content_type_is_a_build_error` (moved from `bodies.rs`) |
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

## Codex Gates 3–4 — `codex/gate4_20261101T171356Z_bfb4870.md` (GPT-6.1 Sol XHigh, read-only, SHA bfb4870)

The first attempt (`codex_gate4_attempt1_quota.log`, not kept in the repository) died when the
session's `/tmp` volume ran out of quota while the coordinator was measuring; it was re-run
unchanged on the same snapshot. Again the reviewer could not build or bind sockets; it used cached
artifacts and MIR probes. It re-reviewed its Gate 2 findings: C4-R-001/002/003/005/006/007 resolved
in source; C4-R-004 partially (invalid punycode labels — fixed in `2fd2e6a`, after the snapshot).

| ID | Sev | Disposition | Evidence / fix | Regression test |
|---|---|---|---|---|
| C4-R-008 | high | CONFIRMED (reproduced) | An `https_only` client refusing a downgrade redirect produces a reqwest error whose *nested* source keeps the redirect target URL, query included; `without_url()` strips only the outer one, so a sensitive query leaked through `Error::source()`. Fix: `source()` is `None` for endpoints with any sensitive URL part; `transport()` remains the explicit, documented-unredacted accessor. The test fails on the pre-fix `error.rs` and passes after. | `tls.rs::c4_r_008_redirect_error_chain_does_not_leak` (loopback HTTPS with a test CA), `tls.rs::non_sensitive_endpoint_keeps_its_source_chain` |
| C4-R-004 (residual) | medium | CONFIRMED, fixed in `2fd2e6a` | `http://xn--55555577.example` accepted by the macro, rejected by IDNA in `url` → constructor panic. Declared bases reject `xn--` labels; a runtime base or label that fails IDNA is a build error. | `ui/base_invalid_port.rs` (`Puny`), `composition.rs::runtime_base_with_invalid_punycode_is_an_error` |
| C4-R-009 | medium | CONFIRMED → contract amended | `maybe_*` takes exactly `Option<T>`; SEMANTICS claimed `IntoValue` conversions. Generalizing the signature makes `None` ambiguous, so SEMANTICS §3 and D3 now state the exact contract. | `ui/conversions_are_closed.rs` (`maybe_cursor(Some("x"))` and `maybe_cursor("x")` are type errors), `values.rs::v07_maybe_is_exact` |
| C4-R-010 | medium | CONFIRMED | No test distinguished draining from buffering for `()`. Added: a compile-time assertion that `()` is `DISCARD`, the DISCARD path receiving 0 of 1,000,000 bytes, and a truncated body (announced 1000, sent 10) failing as a transport error under `()` — proving the body is read to its end. Memory use itself is not measured. | `representations.rs::unit_and_discard_representations_drain`, `unit_drain_detects_truncated_body` |
| C4-R-011 | medium | CONFIRMED, measured | Sensitivity fixpoint re-composed every chain per newly withheld value (cubic). Replaced by one composition pass recording slot memberships plus a worklist; slot lookup by map. Same-machine `cargo check` (incremental, 3 runs) for an N-value chain ending in `authorization`: before 0.32 s / 1.73 s / 9.74 s, after 0.12 s / 0.17 s / 0.35 s for N = 400 / 800 / 1600. | `PERF.md` §2; behaviour unchanged (`sensitivity.rs` all pass) |
| C4-R-012 | medium | CONFIRMED | `GATES.md`/`PERF.md` were not in the reviewed snapshot (added later); triage cited a non-existent `base_forms` test (corrected above); non-JSON representation tests lived in the JSON-gated `bodies` target (moved to `representations.rs`, which runs in every feature combination). Feature runs are package-isolated (`-p concord`) in `scripts/verify.sh`. | `GATES.md` final run |
| C4-R-013 | low | CONFIRMED | Host validation (IP base, length) ran in `finish`, after path/query/headers, contradicting SEMANTICS §6. `Parts::host()` now runs right after the labels. | `composition.rs::c4_r_013_build_error_precedence` |

Other remarks adopted: `duplicate_names.rs` hid the duplicate-value check behind earlier errors
(split into `ui/duplicate_value.rs`); `no_query_string_when_nothing_survives` had no query
contributions (now uses fixtures whose contributions are absent or removed); Unicode hole names
are rejected (documented in SEMANTICS §1 via the hole grammar).

## Claude test-matrix audit — `claude/matrix_audit_1039f13.md` (Opus 5.5 medium subagent, SHA 1039f13)

43 of 55 PASS rows were clean. Problems and dispositions:

| Row | Finding | Disposition |
|---|---|---|
| L01 / V05 | UI suite failed in the auditor's run: two rustc-generated diagnostics (`option_has_no_wire_spelling`, `vec_needs_each`) showed different `help:` wording; matrix said "34 cases" | Not reproduced by the coordinator on rustc 1.97.1 in a fresh target directory (`-p concord --test ui` and `--workspace`). These two cases assert rustc's own wording and are recorded as toolchain-sensitive (as on 1.85). Count corrected. |
| V11 | `duplicate_names.rs` masked the duplicate-value error; `scope s { GET send(); }` asserted nothing; built-in collisions untested | split (`ui/duplicate_value.rs`), line removed, added `ui/builtin_collisions.rs`, `ui/reserved_value_names.rs` |
| V09 / V03 | clone retention, declaration order, "evaluated even if overridden", `send` not re-evaluating were untested | `values.rs::v03_v09_initializer_order_and_clone_retention` |
| M05 | transport half could not detect a retry | `representations.rs::m05_transport_failure_is_one_attempt` (server counts exactly one connection) |
| C04 | 63-byte accept and 253-byte host limit untested | `composition.rs::dns_length_limits` |
| C13 | base trailing slash without generated segments; runtime port 0 accepted | spec clarified (SEMANTICS §4), port 0 rejected at runtime; `composition.rs::base_path_without_generated_segments_is_unchanged` |
| R04 | "not buffered" not discriminated | see C4-R-010 |
| S05 | build errors' `source()` unasserted | `sensitivity.rs::build_errors_have_no_source` |
| gaps | `each` over `Option` under `override`; `Decoder` status handling; reserved headers; header value policy; closed conversions; name normalization | `composition.rs::each_over_option_under_override`, `representations.rs::decoder_applies_status_handling`, `ui/reserved_headers_all.rs`, `composition.rs::header_value_policy`, `ui/conversions_are_closed.rs`, `values.rs::v12_borrowed_string_and_literals`, `sensitivity.rs::safety_list_normalization_is_exact` |

## Codex final review — `codex/gate5_20261101T173328Z_33a88d9.md` (GPT-6.1 Sol XHigh, read-only, SHA 33a88d9)

Re-review of earlier findings: C4-R-004 residual, C4-R-009, C4-R-011, C4-R-013 resolved;
C4-R-008, C4-R-010, C4-R-012 partially. No blocker; one high-severity residual.

| ID | Sev | Disposition | Evidence / fix | Regression test |
|---|---|---|---|---|
| C4-R-008 residual | high | CONFIRMED (reproduced) | The metadata-based rule (hide the chain only when the URL has sensitive parts) missed a sensitive *header* reflected by the server into the redirect target, which the nested reqwest error keeps. Fix: `Error::source()` is always `None`; `transport()` / `body_source()` are the explicit, documented-unredacted accessors. The new test fails on the reviewed `error.rs` and passes after. | `tls.rs::c4_r_008_header_reflected_into_redirect_does_not_leak` |
| C4-R-014 | medium | CONFIRMED | Lockfile lacked the TLS test dev-dependencies (committed in `12e25c9`, after the snapshot). `scripts/verify.sh` and CI now pass `--locked` and fail if tracked files change during verification. | `GATES.md` locked run |
| C4-R-012 residual | medium | CONFIRMED | GATES table was filled after the snapshot (`f51c66a`); MSRV now covers `representations` and `tls`; MSRV UI recounted (35/41 identical); PERF's claim that both comparison crates have wire tests corrected (the baselines' own tests stayed in the research scratch crate); STATUS's source-chain sentence updated. | `GATES.md`, `PERF.md`, `STATUS.md` |
| C4-R-010 residual | medium | CONFIRMED | Earlier tests could not reject "read everything, then discard". `checks/heap` (own crate: a counting global allocator needs `unsafe`, which the workspace forbids) measures peak heap: 64 MiB body → `()` 0.75 MB, `Binary` 137 MB; making `()` buffer (DISCARD = false) fails the test (136 MB). | `checks/heap/tests/drain.rs` |
| C4-R-015 | low | CONFIRMED | The representation's content type was validated in `finish`, after host/path/query/headers. Now validated in `encode_body` (body stage). | `representations.rs::content_type_error_precedes_path_error` |
| C4-R-016 | low | CONFIRMED → spec corrected | `Clone` is required for client/scope values and for values of body-less endpoints (which derive `Clone`), not for endpoints with a body. SEMANTICS §3 now states exactly that. | — |
