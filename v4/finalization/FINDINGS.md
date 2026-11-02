# Findings and dispositions — final V4

Every substantive observation from the fresh-use trials and the independent reviews, with its
disposition: FIXED (commit), DOCUMENTED (where), KEPT (by design, with the reason), or NOT A
CONCORD DEFECT (harness, brief or fixture). Trial reports: `trials/terra-xhigh.md`,
`trials/sonnet.md`; review reports: see `MODEL_RUNS.md`.

## Fresh-use trials (candidate `389392f`)

| Id | Source | Observation | Disposition |
|---|---|---|---|
| U1 | Terra T6, Sonnet T6 | Two spellings of one header on the **same** owner: the run-time `Build` error said "an enclosing owner already sets" it | FIXED `23c21d6`: the error names both declarations (`` `authorization` on `Keys.duplicate` sets a header that `AUTHORIZATION` on `Keys` already sets ``); asserted by `native_keys::identity_errors_are_deferred_build_errors` |
| U2 | Sonnet T6 | A collision involving a native constant is found only by `prepare()`, also when the second value is absent (names are compared before values); DIAGNOSTICS showed only the compile-time variant | DOCUMENTED `3b5c5ed`: DIAGNOSTICS "Setting an inherited key again" (run-time form, message, "prepare every endpoint in a test"), AUTHOR_GUIDE §4.8. The deferral itself is F06 |
| U3 | Sonnet T6 | The fragment error gave a byte offset but not the byte | FIXED `3b5c5ed`: `rejects (0x0A at byte 1)`; `ui/header_literal_value.stderr` |
| U4 | Sonnet T3 | Generated docs of `set_x`/`maybe_x` for an optional body did not say that `set_x(None)` on an `Option<T>` body sends JSON `null` while `maybe_x(None)` sends no body | FIXED `3b5c5ed`: the generated docs say `set_x` always sends `value` encoded by the request format (even `null` or nothing), `maybe_x(None)` sends no body (generic wording: no rule from the type's spelling) |
| U5 | Sonnet T2, T4 | A `String`/`Vec<u8>` payload left under a JSON default is JSON-encoded without an error | DOCUMENTED `3b5c5ed` (QUICKSTART, next to the exceptions); already AUTHOR_GUIDE §5.2. KEPT as semantics (F03: the format, not the type spelling, decides the wire) |
| U6 | Sonnet T5, T10 | Messages and generated docs name a native key as written (`X_NOTES_TENANT`), the wire uses the name | DOCUMENTED `3b5c5ed` (AUTHOR_GUIDE §4.8). KEPT: the spelling is what the author can find in the declaration |
| U7 | Sonnet T5 | `HeaderName::from_static("X-Notes-Tenant")` is rustc's `E0080` from `http` | DOCUMENTED `3b5c5ed` (AUTHOR_GUIDE §4.8, DIAGNOSTICS rough edges). `http`'s const check, not Concord's |
| U8 | Sonnet T7 | Under a renamed dependency the compiler notes still say `concord::Encode<T>` | DOCUMENTED `3b5c5ed` (MIGRATION, renamed dependency). The notes are static `on_unimplemented` text |
| U9 | Sonnet T1 | `accept: */*` is on the wire (reqwest's default) but not in the prepared request | DOCUMENTED `3b5c5ed` (QUICKSTART) |
| U10 | Sonnet T8 | `into_response()` consumes the error; `response.bytes()` fails with `reqwest::Error`, which does not convert into `concord::Error` | DOCUMENTED `3b5c5ed` (CALLER_GUIDE §6) |
| U11 | Sonnet T9 | `RequestBuilder::from_parts(..).header(..)` appends to a declared header; `headers_mut().insert` replaces | DOCUMENTED `3b5c5ed` (CALLER_GUIDE §7) |
| U12 | Sonnet T5 | A scope handle cached across a token rotation keeps the old token | KEPT: handles are snapshots (CALLER_GUIDE §3, RECIPES `Session`) |
| U13 | Terra, Sonnet setup | A copied `Cargo.lock` fails `--locked` in a new workspace until regenerated offline | NOT A CONCORD DEFECT (cargo; trial brief) |
| U14 | Sonnet setup | `examples/riot/tests/support/mod.rs` ends with Riot helpers, so a verbatim copy elsewhere does not compile | NOT A CONCORD DEFECT (example test support; the brief said to copy it) |
| U15 | Terra T3, Sonnet T1–T10 | Task ambiguities (PATCH response, bare or prefixed token, "change the default", which collision) | Preserved in the reports; no change |
| U16 | coordinator, while triaging U1–U11 | Stale prose in `DIAGNOSTICS` rough edges ("written in place") and stale spellings (`json b: T` in the AUTHOR_GUIDE table, `form credentials: Login` in `Form`'s docs, "implement `ToHeaderValue`" in DIAGNOSTICS) | FIXED `4e01aa4`, `663dbe9`, `3b5c5ed` |

## Independent final reviews

Pending (frozen candidate, Sol XHigh executable audit and Opus semantic review).
