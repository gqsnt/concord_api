# V3 → V4 ledger

What happened to each V3 behaviour. Evidence for the V3 side (source paths, line ranges and tests
run at `a492d36`) is in `evidence/research/claude_v3_auditor.md`; V4 evidence is in
`TEST_MATRIX.md`. No V3 code was copied: V4 is a fresh implementation.

## Kept (semantics, re-implemented)

| V3 behaviour | V4 | V4 test |
|---|---|---|
| `?` is an outer presence layer independent of `Option<T>` (VALUE-01) | same | `values.rs::v05_*`, `v06_*` |
| Initializers evaluated once per owner construction; never on clone/send | same; initializers may now be any expression | `values.rs::v03_*` |
| Required scope/endpoint values positional | same, and root values too (no builder) | `ui/missing_required_argument.rs` |
| Closed conversions (identity, `&str`/`&String` → `String`) | same | `ui/newtype_is_not_converted.rs` |
| Immutable snapshots, consuming setters | same (`Arc` copy-on-write) | `values.rs::v10_*` |
| Bare names bind the current owner; `client.x`/`scope.x` qualified | same | `composition.rs::c05_*`, `ui/ancestor_needs_qualification.rs` |
| Duplicate singleton clauses are errors | same | `ui/duplicate_statement.rs` |
| Introducing an inherited key is an error; override/remove need a target | same | `ui/duplicate_inherited_key.rs`, `ui/override_without_target.rs` |
| Suppressed contributions are never encoded | same | `composition.rs::c08_*` |
| Header names case-insensitive, query keys case-sensitive | same | `composition.rs::c09_*`, `query_keys_are_case_sensitive` |
| Relative path templates; each value one segment, encoded once | same, but `/` in a value is encoded, not rejected | `composition.rs::c02_*` |
| Base URL: http(s), host, no userinfo/query/fragment; `/root` ≡ `/root/` | same, and overridable at runtime | `composition.rs::c13_*` |
| DNS labels validated, lowercased, deepest-first | same | `composition.rs::c04_*` |
| Sensitive values sent but withheld; sticky key sensitivity | same, plus a fixpoint so withheld values never appear elsewhere | `sensitivity.rs` |
| Liveness: every value must be used | same (no rate/pagination uses exist any more) | `ui/unused_value.rs` |
| Generated-name collisions are errors | same | `ui/method_collisions.rs` |
| `Option<T>` output has no status meaning without a declaration | same; no `absent on` either | `bodies.rs::r06_*` |

## Rethought

| V3 | V4 | Why |
|---|---|---|
| ordered occurrence patch program (`program.rs`), `replace` vs `when_present` | keyed slots; `override` (absent keeps) + `remove` | `HeaderMap` groups by name; one conditional operation is enough (D4) |
| five value controls (`set`, `replace`, `set_if_some`, `omit`, `reset`) | `set_x`, `maybe_x` | `maybe_x` is exact; the rest is ordinary Rust (D3) |
| `RequestAtom`/`AtomEncoder`/`via`/`concat` | `Display` + templates | one familiar trait; newtypes decide spellings (D6) |
| representation defaults, `explicit` fences, response drivers, `PreparedBody` | `Encode`/`Decode` traits named per endpoint | no inheritance concept, no replay machinery (D7) |
| Empty (strict) / Discard | `()` drains any 2xx body | `-> ()` means "no typed output" (D7) |
| substring name-based redaction | exact-name safety list | substring rule redacted `sessionless-mode`, would redact `page_token` (D8) |
| root `builder()` with accumulated `ClientBuildError` | positional `new(http, ...)` | compile-time instead of runtime (D3) |
| fixed base URL | `base` default + `with_base_url` | tests and staging (D5) |
| reserved public headers (Authorization, Cookie, Accept, Content-Type...) | ordinary; only framing headers reserved | existed for removed owners; C10 |

## Dropped (outside the V4 product)

Credentials, auth placements, challenges, `SecretString`, token refresh, rate-limit registries and
attachments, retries and `effect`, pagination and traversal terminals, operation identities,
deadlines, cancellation, call caps, managed HTTP configuration, `configure_runtime`, runtime
endpoints, unbound intents, `contract()`/inspection export, `expose accepted_bytes`, `status` sets,
`absent on`, `path`/`subdomain` `replace`/`off`, `query off`, retired-V2 migration diagnostics.
Deleting these is the goal, not a regression (guide 05, "Controlled comparisons").
