# Maintenance — reported quota on every result (SDK22 NASA NeoWs)

Date: 2026-11-07. Baseline: `21faa21e` (author `0c8444d6`, fresh consumer `21faa21e`). Role: independent
maintainer; I did not write the SDK or its first consumer. Live provider calls: **NOT_RUN** — every
test runs against `examples/testserver` on loopback. Nothing committed (the coordinator commits).
Model, from harness system metadata: `claude-opus-5-5`; reasoning effort is not exposed to this run.

## Ticket and provider facts

api.nasa.gov returns `X-RateLimit-Limit` and `X-RateLimit-Remaining` "on every API response"
([NASA Open APIs](https://api.nasa.gov/), "Web Service Rate Limits" and "How Do I See My Current
Usage?", read 2026-11-07 from the live static asset `https://api.nasa.gov/assets/html/authentication.html`
and its byte-identical source in `nasa/api-docs`; exact citation, hash and the list of
undocumented points in `PROVENANCE.md`). The application needs the remaining quota after every
success and every failure, the quota refusal included, without exposing the key, and with a clear
absent/unparsable outcome.

Documented and used: the two header names; the decimal example (`998` after two requests against
1,000); default 1,000/hour that "may vary by service"; per-key accounting across api.nasa.gov;
rolling hourly reset; `DEMO_KEY` limited per IP (30/hour, 50/day). **Not documented, so not
asserted:** the status/body of a key or quota refusal (tests use synthetic 403/429), that a refusal
carries the headers (inferred from "every API response"; `Absent` otherwise), what the headers
report under `DEMO_KEY`, any reset or retry header.

## What changed for callers

| Result | Before | After |
| --- | --- | --- |
| Any failure (`send()` or `send_metered()`) | `Error { kind, status }` | `Error { kind, status, rate_limit }` — the failed response's quota for `Status` (403/429 refusals), `Metadata`, `Decode`, `Limit` and mid-body `Transport`; both `Absent` for `Build` and no-response `Transport` (`status` is `None`) |
| Success via `send()` | bare `T` | unchanged: bare `T` |
| Success via `send_metered()` (new; `use concord_sdk_nasa_neows::SendMetered`) | — | `Metered<T> { value, rate_limit }`, every endpoint, same single request, 4 MiB bound and errors as `send()` |

`RateLimit { limit: Reported, remaining: Reported }`; `Reported` is `Value(u64)`, `Absent` (no
such header, or no response) or `Unparsable` (present but not exactly one header field of ASCII
decimal digits fitting `u64`: repeated, empty, `+5`, `-1`, `1.5`, `1e3`, `0x10`, non-ASCII,
overflow). Surrounding SP/HTAB is not part of the value. **No header text is retained**, so a
header that echoes the key cannot reach the value, `Debug` or `Display`; `Display` is unchanged,
`Debug` additionally prints the two parsed numbers. `RateLimit::from_headers` is public for
responses obtained natively (`prepare()` + reqwest). The SDK still installs no pacing, retry or
reset prediction.

### Design decision: additive terminal, not a new `send()` output

Concord's single point for "what every endpoint returns" is the client's send policy
(`SendWith::Output`). Changing `Bounded`'s output to `Result<Metered<T>, Error>` would put quota
on `send()` itself with one edit — but it changes the caller contract of every endpoint. I measured
that counterfactual (temporary edit, then restored; `src/lib.rs` hash re-verified):
`tests/first_use.rs` stops compiling with **12 errors** (8 E0609 field access through the wrapper,
4 E0308 at `report.feed = Some(feed)`, `report.details.push(detail)` ×2, `report.pages.push(page)`),
`tests/workflows.rs` with 8 E0609, the README doctest with 6 E0609; `tests/protocol.rs` still
compiles. A `Deref` would remove the E0609s but not the four E0308s. Since the first consumer must
be kept with its assertions, and the smallest coherent change keeps the current design, I kept
`send()` and added one blanket terminal. Cost: success quota is opt-in and needs a trait import,
which the fresh consumer explicitly valued not needing (`FIRST_USE.md`); nothing makes a `send()`
caller notice it. If the owner prefers a single terminal, the breaking variant is a one-line
policy change plus the call-site edits listed above — an owner decision, not taken here.

`SendMetered` is implemented for `E: Endpoint<T> + Sends<Result<T, Error>>` — endpoints whose
generated `send()` returns this crate's result — rather than for every `Endpoint<T>` as Concord's
own `Endpoint` doc sketch does; the unscoped form would also attach NASA quota parsing and the
NeoWs error to any other Concord client's endpoints. A `compile_fail,E0599` doctest pins this
(probe confirmed E0599 for a foreign client, and that the foreign `send()` still compiles).

## Every location touched

| File | Location | Change |
| --- | --- | --- |
| `src/lib.rs` | crate doc (l. 3–5) | quota on every result |
| `src/lib.rs` | imports (l. 8–11) | `HeaderMap`, `HeaderName`, `Sends`, `Future` |
| `src/lib.rs` | l. 78–79 | two header-name constants (the only place the names are stated in code) |
| `src/lib.rs` | `Reported` + the one parser `Reported::read` (l. 81–120), `RateLimit` + `from_headers` (l. 122–146), `Metered<T>` (l. 148–155) | new types |
| `src/lib.rs` | `Error` doc + `rate_limit` field (l. 157–167) | new public field; doc no longer claims "no headers" |
| `src/lib.rs` | `From<concord::Error>` (l. 169–179) | quota from `concord::Error::head()` |
| `src/lib.rs` | `Bounded::send` (l. 195–200) | delegates to the shared `metered` path |
| `src/lib.rs` | `SendMetered` + blanket impl + `metered()` (l. 202–240) | new terminal; the single send path |
| `README.md` | error paragraph, native paragraph, new "Reported quota" section + compiled example | stale "no headers" claim corrected; usage |
| `PROVENANCE.md` | new "Quota headers" section | citations, hash, documented vs undocumented |
| `tests/rate_limit.rs` | new | focused regression (5 tests) |
| `tests/quota_workflows.rs` | new | consumer test (2 tests) |
| `MAINTENANCE.md` | new | this file |

Not touched: the `concord::api!` declaration (no endpoint, scope or client statement changed), the
response models, `Cargo.toml`, root `Cargo.toml` (already a member), `Cargo.lock`, fixtures,
`tests/first_use.rs`, `tests/protocol.rs`, `tests/workflows.rs` (SHA-256 identical to baseline),
`FIRST_USE.md`, `FRICTION.md`, every shared crate. No dependency added.

**Facts stated in several places.** (1) "Read the quota from the head" has two call sites —
`metered()` reads `Received::head()` before decoding, `From<concord::Error>` reads
`concord::Error::head()` — because Concord exposes the success head and the failure head through
different APIs; both call the one `RateLimit::from_headers`. (2) The quota semantics are prose in
four places: rustdoc of `RateLimit`/`Reported`/`Error::rate_limit`/`SendMetered`, README "Reported
quota", PROVENANCE, and this file; NASA's numbers (1,000/hour, DEMO_KEY 30/50 per IP) appear in
README and PROVENANCE only. (3) Two pre-existing claims became false and had to be edited: the
`Error` rustdoc ("retains no … headers") and the README ("Debug … contain no … headers").

## Did the compiler or the tests catch omitted edits?

Mutation runs on the finished `src/lib.rs` (each restored; hash verified after every run):

| Omitted / wrong edit | Caught by |
| --- | --- |
| error path keeps `RateLimit::default()` | `quota_workflows` (2/2 fail), `rate_limit::failures_report_quota_at_every_stage_after_the_head`, `…absent_and_unparsable…` |
| success path keeps `RateLimit::default()` | `quota_workflows` (2/2), `rate_limit::success_results…`, `…absent_and_unparsable…` |
| `remaining` reads the limit header | `quota_workflows` (2/2), three `rate_limit` tests |
| repeated header fields accepted | `rate_limit::absent_and_unparsable…` only |
| lenient `u64::from_str` (accepts `+5`) | `rate_limit::absent_and_unparsable…` only |
| SP/HTAB trim removed | `rate_limit::native_header_maps…` only — on the wire hyper already strips it, so every loopback test passed |

The 8 pre-existing tests passed under **every** mutation: they predate the feature and cannot
observe it. The compiler forces the `rate_limit` field in the one `Error` struct literal (Rust
rule; not separately exercised). Nothing — compiler or test — caught the two stale doc claims; I
found them by reading. Adding a future endpoint needs no quota edit: the policy and the blanket
`SendMetered` cover it by type.

## Test results (`--locked --offline`, `CARGO_BUILD_JOBS=4`, worktree `target/`)

- Baseline before edits: 8 integration tests + 1 doctest passed.
- After: `cargo test -p concord-sdk-nasa-neows` — **15 integration tests passed** (first_use 2,
  protocol 4, workflows 2, rate_limit 5, quota_workflows 2) and **3 doctests** (2 README
  compile-only, 1 `compile_fail,E0599`); 0 failed.
- `cargo clippy -p concord-sdk-nasa-neows --all-targets -- -D warnings`: clean.
  `RUSTDOCFLAGS="-D warnings" cargo doc -p concord-sdk-nasa-neows --no-deps`: clean.
  `cargo fmt -p concord-sdk-nasa-neows --check`: clean (rustfmt was run only on the two new files).
- Regression coverage: all three endpoints × {success with headers, 403 key refusal, 429 over-limit
  refusal with an unread stalled body, Decode, Metadata, Limit, mid-body Transport} through both
  `send()` and `send_metered()`; absent vs unparsable table on a success and on the 429 refusal;
  no-response `Build`/`Transport` → `Absent`; one request per call; key absent from every
  `Display`/`Debug`/`{:#?}` including a header echoing it; `source()` is `None`.
- Consumer coverage: an application logs the quota after every call of feed → lookup and
  browse → inspect against a synthetic decrementing counter, ending in the 429 refusal that itself
  reports zero; a careful variant stops on a reported zero (no fourth request) but proceeds when an
  intermediary stripped the headers (`Absent` ≠ zero) and then records NASA's refusal. Fixture
  bodies echo the key; the log's `Debug` never contains it.

## Remaining generic duplication

- The success-and-failure metadata capture (prepare → receive → copy from head → bounded decode;
  same projection from `concord::Error::head()`) is hand-written per SDK; this repo already has
  per-SDK `Response<T>`/meta wrappers in github, gitlab, airtable, dropbox, openai, linear, notion,
  google-drive.
- Strict numeric header parsing is generic HTTP work done ad hoc and inconsistently:
  `examples/github-sdk/src/meta.rs` uses `trim().parse::<u64>().ok()`, which accepts `+5` and folds
  absent and unparsable into `None`; this SDK distinguishes them and rejects `+5`.
- The redacted error projection (kind, status, now selected metadata) remains SDK-owned, as the
  author's `FRICTION.md` already recorded.
- A second terminal exists only because `SendWith::Output` is one type per client: metadata added
  after release must either break `send()` or add a parallel terminal with a trait import.

## Provider-specific responsibilities (stay in the SDK)

Header names and their meaning (per key, rolling hourly, varying by service); the strictness
policy and what `Absent` covers; never inferring a reset time, default limit or pacing; not
classifying 403 vs 429 as key vs quota (undocumented); not equating `DEMO_KEY` (per IP) with a
registered key; keeping no header text so a key echo cannot escape.

## Candidate Concord improvements (recorded, not implemented; shared crates are frozen)

1. **Metadata capture in the send path.** A core helper such as
   `Received::decode_limited_capturing(limit, |head| M) -> Result<(T, M), (Error, Option<M>)>` (or a
   `capture` hook on `SendWith`) so one function states "this metadata on success and on failure",
   instead of two call sites over two head APIs.
2. **Additive metadata without breaking `send()`.** Either a generated per-client marker trait (e.g.
   a `NeoWsEndpoint` bound) so an SDK's blanket extension trait is scoped to its own endpoints
   without the `Sends<Result<T, Error>>` trick, or a policy-declared second terminal
   (`send_with_meta()`) generated next to `send()` that needs no import. Concord's `Endpoint` doc
   sketch currently suggests an unscoped blanket impl, which leaks onto other clients.
3. **Typed header readers** on `ResponseHead`, e.g. `decimal_u64(name) -> {Absent, Unparsable,
   Value}`, single-field and digits-only, so SDKs agree on strictness.
4. **Generated docs.** `send()` shows `<Bounded as SendWith<Feed>>::Output` (fresh-consumer
   friction 2); a blanket extension method like `send_metered` is visible only under "Blanket
   Implementations". Linking the concrete output and listing SDK terminals would aid discovery.

## Prior exposure (before starting)

Read before implementing: the capsule `work/roles/w1-sdk22-maintainer-opus/prompt.md`; the
coordinator's instructions and the user/project `CLAUDE.md` shown in context; this package's
`src/lib.rs`, `Cargo.toml`, `README.md`, `PROVENANCE.md`, all fixtures, `tests/first_use.rs`,
`tests/protocol.rs`, `tests/workflows.rs`; Concord sources `crates/concord/src/lib.rs` (crate docs),
`crates/concord-core/src/response.rs`, parts of `error.rs` and `prepared.rs` (`Endpoint`,
`SendWith`, `Sends`, `decode_keeping`), a grep of `codec.rs` and of `concord-macros/src/emit.rs`
for `Sends`; `examples/github-sdk/src/meta.rs` (rate-limit precedent) and the `Response<T>` of
`examples/airtable-sdk/src/lib.rs`; a grep of other SDKs for `Response<T>`/`SendWith`;
`examples/testserver/src/lib.rs` (API and header writing); root `Cargo.toml` and `rustfmt.toml`;
NASA's documentation above. **`FRICTION.md` and `FIRST_USE.md` were read only after the change,
both new test files and the mutation runs were complete and passing.** Reading them afterwards
changed no code: `FIRST_USE.md` had predicted that quota headers were unavailable and recommended
"an explicitly safe projection of quota headers" for a later scheduling task, which is this
change; it also records the consumer's preference for no trait import, the trade-off stated above.
No other agents were used.
