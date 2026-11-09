# M. Testing story for SDK authors — merged ideas
Sources: DJM-claude.md, DJM-codex.md. 8 lane designs → 7 entries: 4 KEEP, 2 KILL, 1 MERGED.

Evidence checked at `ba10189b` in `work/wt/rv-p1` (read-only); "checked" = opened or counted in this merge, "lane estimate" = not re-derived. All savings in this family are test code, which the glue classifier (`src/` only) does not count.

## M1. Publish `concord-test`: the fixture server, the canary and the helpers every SDK copies — KEEP (rank 1)
Sources: DJM-claude.md § M1, DJM-codex.md § M1, DJM-codex.md § M3 (probe half; entry M5).
Fact: none (test infrastructure; no declaration change).
Mechanism: move `examples/testserver` (1,240 `src/` lines) and `checks/canary` (219 lines), both `publish = false` today (checked), into one published dev-dependency with old-name shims; add what SDKs re-write: `strict_client()`/`strict_builder()` (no proxy, no redirects, `retry::never()`), `refused_url()`, the Concord-boundary probes `Canaries::concord_error(&Error)` and `Canaries::receive(prepared)`, `wire(&prepared) -> Recorded` (assert composition without a server), and `TestServer::scripted_with_origin(build)` (bind first, build replies from the bound origin, then accept; FRICTION M4). Codex M3's probe extensions (named secrets, `assert_public`, a cycle guard on source chains, derived-use proofs) live in the same crate.
Declaration: unchanged. Tests, Twilio `examples/twilio-sdk/tests/canary.rs:14-26,44-64` (checked) →
```rust
fn http() -> concord::reqwest::Client { /* 8 lines */ }
fn refused() -> String { concord_example_testserver::refused_url() }
async fn receive<T>(c: &mut Canaries, p: concord::Prepared<T>) -> Result<T, concord::Error> { /* 11 lines */ }
fn concord_error(c: &mut Canaries, e: &concord::Error) { /* 7 lines */ }
// after
use concord_test::{strict_client, refused_url, Canaries};
let native = c.receive(read).await;
```
Call site: n/a (SDK tests). Registry `tests/protocol.rs:35-46` `self_aware` (12 lines, checked) → `concord_test::TestServer::scripted_with_origin(build).await`.
Glue removed: test code. Checked: 55 `tests/canary.rs` files, 13,004 lines, 54 loop `for case in 0..7`; helper copies `fn http`/`http_with` 71 definitions, `fn refused` 53, `fn concord_error` 59, `fn receive` 42; lane measured their bodies at ≈ 1,750 lines. Corrections: the testserver's Nagle delay (Kubernetes FRICTION) is already fixed (`examples/testserver/src/lib.rs:574-576`, commit `e0950f32`), and Salesforce's "~70-line router" (FRICTION F14) is not in current tests (codex is right) — neither is a saving. FRICTION M2/M4/M6 (4/2/2 SDKs).
Counterexample: GitHub resolves hosts and uses its own redirect policy (`github-sdk/tests/support/mod.rs:46-82`, checked) → starts from `strict_builder()`. S3's virtual hosts, `Body::HeadOnly`, the paused clock, gates and the independent SigV4 verifier move unchanged; Registry's stateful upload/token fake stays SDK code.
Simple control: Nager's `tests/canary.rs` loses its `http`/`refused`/`concord_error` helpers (≈ 25 test lines); declaration unchanged.
New concepts: crate `concord-test` (existing `TestServer`/`Reply`/`Canaries`, plus ≈ 6 helpers).  Breaking: none for callers; SDK dev-dependencies renamed (shims meanwhile).
Shares mechanism with: D (the canary becomes the public C9 tool for D1/D2; IDEAS seed D4); J (a capturing subscriber probe for J1).
Screen:
- C3: nothing in declarations. C7: `strict_client()` returns a plain `reqwest::Client`. C8: per-test servers, no global state.
- C9: the canary becomes a public, versioned tool SDK authors run, instead of an internal check.
- Risk: publishing 1,240 server lines (paused clock, gates, body generators) is a stability promise.

## M2. The seven failure shapes as shared scenario data — KEEP (rank 2)
Sources: DJM-claude.md § M2, DJM-codex.md § M3 (scenario remark).
Fact: none; Concord names what every SDK must survive — 2xx, 4xx with an error document, 5xx, malformed body, wrong `Content-Type`, truncated body, refused connection.
Mechanism: `concord_test::Shape::ALL` with `shape.reply(success, problem, echo) -> Reply` and `shape.expected()` (the Concord outcome: `Ok` or `ErrorKind::{Status, Decode, Metadata, Transport}` with its `Delivery`), plus a driver that runs each shape against a fresh server and applies the canary checks. Provider interpretation stays SDK code passed as closures (codex form), not an `Inspect` trait every SDK implements (claude form — that half only moves glue).
Declaration: unchanged. Twilio `tests/canary.rs:28-42` + loop (checked) →
```rust
fn reply(case: usize, status: u16, success: &str, problem: &str, echo: &str) -> Reply { /* 14 lines */ }
for case in 0..7 { /* TestServer::start; with_base_url(if case == 6 { refused() } else { server.url() }) … */ }
// after
concord_test::conformance(&mut c, bodies,
    |base| async move { client(&base).get_message("SM1").await },
    |shape, outcome| assert_twilio_mapping(shape, outcome)).await;
```
Call site: n/a.
Glue removed: test code; `reply()` (Twilio 14 lines, checked) + ≈ 10 lines of loop scaffolding in each of the 54 canary files that loop over 7 cases (checked) → lane estimate ≈ 1,300 lines. The SDK's own result/error probes (Twilio `error()`/`result()`, ≈ 25 lines) stay.
Counterexample: Slack 200 `ok:false`, ClickHouse's exception after a 2xx head, Snowflake 202 — the seven are a floor and SDK shapes are extra replies. Nager has no error schema (its 4xx/5xx are `text/plain`, checked), so the problem body carries its media type. S3 HEAD failures without a body and ClickHouse drained inserts keep their own outcome closures.
Simple control: Nager's `tests/canary.rs` `reply()` and loop collapse to one call.
New concepts: `Shape`, `conformance()`.  Breaking: none.
Shares mechanism with: A (once SDK errors are Concord-shaped, the outcome closure shrinks).
Screen:
- C6: seven named shapes, no paragraph. C8: one server per shape, no shared state.
- C9: the guarantee every SDK claims gets one definition.
- The `Inspect`-trait form fails the "only moves glue" rule (a trait every SDK implements with the same lines); kept in the closure form.

## M3. Tests reach the declared origin: CONNECT proxy with a throw-away CA — KEEP (rank 3)
Sources: DJM-claude.md § M4.
Fact: none new; tests exercise the declared base, scheme and subdomain labels as the provider is addressed.
Mechanism: `concord_test::Origin::start(responder)` runs the test server also as an HTTP CONNECT proxy: for `https://api.twilio.com` it accepts `CONNECT api.twilio.com:443`, mints a leaf certificate, terminates TLS and records `host = "api.twilio.com"`. `origin.client()` = `strict_builder()` + `Proxy::all(origin.proxy_url())` + `tls_certs_only([origin.ca()])` (`tls_certs_only` already used in `crates/concord/tests/tls.rs:83-89`, checked). No DNS override, no `with_base_url`.
Declaration: unchanged. Test, Twilio `tests/support/mod.rs:41-49` (checked) →
```rust
Twilio::new(http(), ACCOUNT, auth).with_base_url(base).unwrap()        // before
let origin = concord_test::Origin::start(move |_| script.next()).await;
let twilio = Twilio::new(origin.client(), ACCOUNT, auth);                // after: the declared https://api.twilio.com
```
Call site: n/a.
Glue removed: tests — 352 `with_base_url` uses (checked) become `origin.client()` (a substitution, not a deletion); host-resolution helpers go (GitHub `tests/support/mod.rs:46-82` ≈ 35 lines, S3 `tests/common/mod.rs:67-81`). `src/`: 18 SDKs hand-write `with_base_url`/`for_fixture` wrappers (checked), some only for fixtures (Twilio `src/lib.rs:89-102`: "Use the loopback test server's URL"). The main value is cross-family: family C can make the base a fixed provider fact without a test-only constructor. FRICTION M1 (testserver cannot do https/subdomain hosts; 5 SDKs).
Counterexample: S3 SigV4 signs `Host` — the test now verifies the real canonical request. The refused-connection shape keeps `refused_url()` with a direct client. Real deployments (self-hosted ntfy, Elasticsearch, ClickHouse, Registry) keep a base setter the declaration states.
Simple control: Nager `tests/support/mod.rs:10-19` (checked) becomes `NagerHolidays::new(origin.client())`; declaration unchanged.
New concepts: `concord_test::Origin`.  Breaking: none in Concord; `concord-test` gains rustls/rcgen dependencies.
Shares mechanism with: C (a fixed base without test-only constructors — what the reverted `controls {}` patch needed).
Screen:
- C7: the caller's `reqwest::Client` stays the only seam (proxy and roots are client settings).
- C8: the redirection lives in the test's client; nothing hidden in Concord.
- C9: tests check the declared URL, not a substituted one.
- Risk: 300+ lines of TLS/proxy test infrastructure with its own failures, and a handshake per test.

## M4. `composition()`: the declaration's wire plan as a reviewed snapshot — KEEP (rank 4)
Sources: DJM-claude.md § M3.
Fact: nothing new — a deterministic text of what each endpoint declares: method, URL template, query keys in wire order, header names with template and sensitivity, formats and media types, limit. Templates only, never values, so the file is safe to commit.
Mechanism: the macro emits `Client::composition() -> &'static str` (a sibling of `validate_header_names()`); `concord_test::assert_composition!(TwilioApi, "tests/twilio.composition")` compares it with a checked-in file; `CONCORD_BLESS=1` rewrites it.
Declaration: unchanged. Test, Twilio `tests/protocol.rs:20-28` (checked) →
```rust
assert_eq!(request.method, "POST");
assert_eq!(request.target, CREATE_PATH);
assert_eq!(request.header_all("content-type"), ["application/x-www-form-urlencoded"]);
assert_eq!(request.header_all("accept"), ["application/json"]);
// after
#[test] fn composition() { concord_test::assert_composition!(TwilioApi, "tests/twilio.composition"); }
```
Call site: n/a.
Glue removed: test code; ≈ 1,200 grep hits of method/target/accept/content-type/authorization assertions in SDK and app tests (checked; lane 1,042 in SDKs), about half restating declared facts → ≈ 500 lines (lane estimate). Protocol tests keep SDK-computed values (form bytes, S3's SigV4 `authorization`).
Counterexample: ClickHouse's `query(..)` keys are chosen at run time → shown as `query(..) params`, the keys stay a protocol assertion; Europe PMC public ids appear as `{id}` placeholders; native header keys are rendered by constant name.
Simple control: optional one test line and a six-line file; Nager's declaration unchanged, its generated surface gains one associated fn (precedent: `validate_header_names()`).
New concepts: generated `composition()`; test macro `assert_composition!`.  Breaking: none (an endpoint named `composition` follows the `validate_header_names` collision rule).
Shares mechanism with: H (the same text is the "method and URL template" doc line); J1 (route); K (query order becomes visible in review).
Screen:
- C4: the file shows exactly what a reader should predict from the declaration. C6: the text must read without docs (risk).
- C9: a snapshot detects change, not error — a wrong path blessed once stays green (codex § M2's independence argument), so it supplements independent wire tests and never replaces them.
- Risk: the text format becomes a compatibility surface (a format change re-blesses 55 files).

## M5. Public canary probes with positive public-field controls — MERGED into M1
Sources: DJM-codex.md § M3.
Promote `checks/canary` (it already has `assert_debug_absent`, `assert_error_absent` over source chains and a wire scan that decodes Basic auth, `checks/canary/src/lib.rs:70,84,130-153`, checked) and add named secrets, shared Concord-boundary probes, `assert_public` (catches over-redaction: Twilio SID/status, Europe PMC ids, a public ClickHouse identifier), host-aware wire receipts, a cycle guard and derived-use proofs for S3's signing secret. Same mechanism as M1's canary half; its remark that provider interpretation stays a callback is M2's form.

## M6. `RequestExpectation`: fluent assertions on `Prepared` and `Recorded` — KILL (removes almost nothing for a new concept)
Sources: DJM-codex.md § M2.
Twilio `tests/protocol.rs:20-34` goes from 15 `assert_eq!` lines to 7 builder lines (≈ 8 net; Salesforce 3–4); the lane expects 1–2 lines on other blocks and notes it may be more surface than `assert_eq!`. Its build-only half (assert a `Prepared` without a server) is M1's `wire(&prepared)`; its independence argument is kept as M4's risk.

## M7. Canary-filled models (`fill`, `assert_withheld`, derive-emitted `SHOWN`) — KILL (C9: the test cannot fail for the case it claims)
Sources: DJM-claude.md § M5.
`SHOWN` is emitted by D1's derive from the unmarked fields, so a field added later without `#[sensitive]` lands in `SHOWN` and its canary is excused: "fails when a field is added unmarked" is tautological. Under D1's variant D4 new fields are withheld anyway, so the test only re-checks the derive, which Concord's own corpus test covers. The serde filler for flatten/untagged/validated types is also clever code that confuses authors (C6), and S3's XML models are outside serde.
