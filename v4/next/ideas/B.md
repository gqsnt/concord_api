# B. Results — merged ideas
Sources: AB-claude.md, AB-codex.md (family B parts; AB-claude.md § A3 is counted in A as A4 and lands in B3). 8 lane designs → 6 entries: 4 KEEP, 2 KILL, 0 MERGED.

Evidence checked at `work/wt/rv-p1` `ba10189b`: 21 SDK crates define `pub struct Response<`
(AB-claude says 23 `Response`-style structs, IDEAS 20). FRICTION B1 "no core value-plus-metadata
result" is reported by 18 SDKs, B2 "`send()` rustdoc shows a projection" by 9, B3 "envelope
projected by hand, `_` binds the whole envelope" by 6, B5 "status-dependent outcomes" by 3.

## B1. `concord::Response<T>` in core, plus an opt-in core `KeepHead` policy — KEEP (rank 1)
Sources: AB-claude.md § B1, AB-codex.md § B1, AB-codex.md § A1 (`concord::KeepHead`)
Fact: every answer has a head; the value and the head it came with are one core type, not 21 copies.
Mechanism: library only. `concord::Response<T, R = ()> { value, meta: ResponseMeta, rest: R }` with `From<(T, ResponseMeta)>`, `map`, `into_value`, `into_parts`, and `Received::decode_response()`; `Debug` prints the head and withholds the value without a `T: Debug` bound.
Variants: `rest: R` for B3 (claude) vs plain `Response<T>` with B3 using its own `EnvelopeResponse` (codex) · optional core `concord::KeepHead` (an ordinary `SendWith`, codex) so a stateless SDK writes `send concord::KeepHead -> Result<concord::Response<_>, Error>;` instead of defining its own policy.
Declaration: PokeAPI (`lib.rs:81`, `lib.rs:101-108`):
```rust
-       send KeepHead -> Result<Response<_>, Error>;   // + pub struct Response<T> { value, meta } (8 lines)
+       send KeepHead -> Result<concord::Response<_>, Error>;
// Notion client.rs:9-22 (14 lines) → `pub use concord::Response;`
```
Call site: callers unchanged (`r.value`, `r.meta.status()`). In policies: `let (v, m) = received.decode_with_meta().await?; Ok(Response { value: v, meta: m })` → `Ok(received.decode_response().await?)`.
Glue removed: claude ≈300–350 lines over the wrapper SDKs; codex ≈27 net in Notion and Kraken and warns that holders with extra fields do not qualify. Checked: 21 crates define `pub struct Response<`, but Slack's carries `warning` and `response_metadata` (`lib.rs:29-40`) and Stripe's carries its own meta (`send.rs:135-142`), so a realistic range is ≈150–350. With the `KeepHead` variant plus A1, the ≈20 stateless head-plus-document policies (B5's ≈650 lines) also go, opt-in.
Counterexample: PokeAPI keeps the final URL in its meta (`lib.rs:90-99`), which `ResponseMeta` deliberately omits (a URL is unredacted): it stays as is. Slack/Cloudflare body diagnostics → B3 `rest`, never dropped. No-output endpoints → `Response<()>` from a drained body; streams → the head plus the opened reader, not collected.
Simple control: unchanged (Nager keeps `HolidayResponse { holidays, meta }`, Frankfurter `RateResponse`; adoption is optional).
New concepts: `concord::Response<T, R>`, `Received::decode_response`, optional `concord::KeepHead`.  Breaking: none in Concord; an adopting SDK changes the defining crate of a public type, and inherent accessors (Anthropic `request_id()`, `lib.rs:129-134`) need an extension trait or B2.
Shares mechanism with: A1 (`Failure<D>` holds the same `ResponseMeta`), F2/F3 (concord-pagination reaches the value inside `Result<Response<T>, E>` without a per-SDK impl), E (`meta.operation()`).
Screen:
- C1: no new word.
- C3: controls unchanged.
- C6: "the value plus the head it came with".
- C7: `into_parts` and native paths remain.
- C8: adds no work.
- C9: canary that `Debug` never prints the value or a header value; compile test with a non-`Debug` payload.

## B2. `meta Name(header("..") field?: T, ..);`: declared response header fields — KEEP (rank 2)
Sources: AB-claude.md § B3
Fact: this provider answers with these headers, meaning these typed values: request id, rate-limit counters, replay flag, API version, page totals. The charter lists rate-limit header names as a declaration fact.
Mechanism: owner statement mirroring input placements. It generates `pub trait Name`, implemented for `ResponseMeta`, `ResponseHead<'_>`, `Failure<D>` and `Error<D>`, with one lazy, strict accessor per field (`Result<Option<T>, FieldError>`, `T: FromStr`, `concord::field` rules: absent → `None`, repeated or invalid → error). `each` reads multi-line headers; `#[sensitive]` withholds a field.
Declaration: Stripe (replaces `send.rs:79-133`):
```rust
        meta StripeMeta(
            header("request-id") request_id?: String,
            header("idempotent-replayed") idempotent_replayed?: bool,
            header("stripe-should-retry") should_retry?: bool,
            header("stripe-rate-limited-reason") rate_limited_reason?: RateLimitedReason,
            header("stripe-version") stripe_version?: String,
        );
```
Call site:
```rust
- if let Some(id) = &response.meta.request_id { .. }
+ use stripe::StripeMeta;  if let Ok(Some(id)) = response.meta.request_id() { .. }
+ if let Some(f) = error.provider() { f.rate_limited_reason()?; }   // same accessor on failures
```
Glue removed: claude ≈450–500 net over 12 SDKs (Stripe, S3, Slack, Registry, DigitalOcean, Cloudflare, GitLab, GitHub, Anthropic, QuickBooks, Dropbox, ClickHouse) — lane estimate. Checked: Stripe `send.rs:79-133` is 55 lines for 5 headers → ≈40 net after a `FromStr` for `RateLimitedReason`, as claimed.
Counterexample: GitHub/GitLab `Link` resolves relative targets against the response URL (GitLab `meta.rs:74,100`) → not `FromStr`, stays hand-written. S3 `x-amz-meta-*` is a family of names → stays (`xml::metadata_headers`). Stripe and S3 silently drop invalid values today; strict parsing changes that (a behavior change to record).
Simple control: unchanged.
New concepts: `meta Name(..)`, one generated trait per statement, `header(..)`/`each` output placements.  Breaking: none in Concord; adopters turn public fields (`meta.request_id`) into methods (`meta.request_id()`), and callers import the trait (the H09c misses were import-path errors, FRICTION G1).
Shares mechanism with: E (rate-limit headers a limiter reads: the charter's fact/behavior pair), J (request id for log spans), D (`#[sensitive]` field), F (page totals in headers), A1 (accessors on `Failure<D>`), E2/F2 (the same placement syntax as inputs, mirrored).
Screen:
- C1: header names and types are provider facts.
- C2: the parse rule is Rust `FromStr`.
- C3: opt-in.
- C4: the nearest owner applies.
- C5: a non-`FromStr` `T` is reported at the field.
- C6: one sentence.
- C8: lazy parse, no hidden work.
- C9: canary that `ResponseMeta` `Debug` still prints names only; strict-parse tests (repeated, `+5`, overflow).

## B3. `envelope E<_>;`: the success envelope as a declared fact; `_` is the value — KEEP (rank 3)
Sources: AB-claude.md § B2, AB-codex.md § B2, AB-claude.md § A3 (via A4)
Fact: on this API a 2xx body is `E<value>`. The envelope may refuse (Slack `ok:false`, Kraken `error`, MediaWiki `error`, QuickBooks `Fault`) and carries members the caller needs (warnings, cursor, diagnostics).
Mechanism: owner statement `envelope E<_>;`. The wire type is `E<output>`, using the macro's existing `with_output` substitution (`emit.rs:2090`). After decoding, Rust opens it: `impl concord::Envelope for E<T> { type Value; type Rest; type Error; fn open(self) -> Result<(Value, Rest), Refusal<Error>> }`. `Refusal::Provider(d)` becomes `concord::Error<D>` (kind `Refused`, the real status) when `Error` is the declared `error D` (A1); `Refusal::Malformed` becomes `Decode`. Generated `send()` returns `Response<Value, Rest>`; `prepare()?.send()` still returns the raw envelope.
Variants: codex has no refusal and no hand `open`: `#[derive(concord::Envelope)]` with `#[concord(value)]` on the field, `EnvelopeResponse<T, E>` keeping the whole envelope and borrowing `value() -> Option<&T>` (Linear's data beside errors).
Declaration: Kraken (`lib.rs:32-47`):
```rust
        response Json;
        limit RESPONSE_LIMIT;
+       error ErrorDocument;                  // A1: non-2xx bodies
+       envelope Envelope<_>;                 // 2xx body; its `error` list is the same ErrorDocument
-       send policy;
+       send policy -> Result<Response<_>, Error>;
-           GET pairs(query pair?: String) -> Envelope<Pairs> { path "AssetPairs"; }
+           GET pairs(query pair?: String) -> Pairs { path "AssetPairs"; }
```
plus a 6-line `open` (non-empty `error` → `Refusal::Provider`, no `result` → `Malformed`) replacing `policy.rs:156-166` and `error.rs:117-140`.
Call site (Slack):
```rust
- let r = slack.conversation_info("C1", ConversationOptions::default()).await?;   // wrapper, lib.rs:210-232
+ let r = api.conversations_info("C1".into()).send().await?;
  r.value.channel; r.rest.warning; r.meta.status();
```
Glue removed: claude ≈300 net in 6 SDKs (Slack ≈80, Cloudflare ≈120, Kraken ≈15, MediaWiki ≈20, Linear ≈25, QuickBooks ≈45) plus the in-2xx refusal ≈130 net in 5 SDKs; the two overlap (Slack, Kraken, Cloudflare, MediaWiki, QuickBooks), so ≈300–430 in all. Codex's view-only variant: ≈5–8 net in Linear. Checked: Cloudflare `wire.rs:89-176` (88 lines: `Single`, `Paged`, `Raw`, two `Deserialize` impls) and `policy.rs:10-45`, `:156-240` (two holders, two `SendWith` impls) support ≈100–120 net (codex counts ≈90–105 on the same code); Slack's envelope router `lib.rs:235-279` is 45 lines. It also unlocks Slack's `Client` wrapper (`lib.rs:118-139`, family G), not counted.
Counterexample: Cloudflare's success-first lazy parse → `result` kept as `RawValue`, parsed in `open` (model code). Linear partial data → `errors` stay in `Rest` while `data` exists (or the codex view). QuickBooks' `Fault` replaces the entity rather than wrapping it → a custom `Faulted<_>` envelope, saving little. ClickHouse's header-signalled in-stream exception → not covered, stays SDK behavior. `-> stream` and no-output endpoints are not enveloped. An API whose `Rest` differs per endpoint gets `Option` fields or an endpoint-level `envelope` line.
Simple control: unchanged (none of the three has an envelope).
New concepts: `envelope E<_>;`, `concord::Envelope`, `Refusal<E>`, `Received::open`, `Response::rest`, `ErrorKind::Refused` (codex: the derive, `#[concord(value)]`, `EnvelopeResponse`).  Breaking: adopters' outputs change `Envelope<Pairs>` → `Pairs`, and `Sends<O>` bounds with them (Cloudflare `pagination.rs:81`); SDKs with separate 2xx and non-2xx variants (Kraken `Api`/`Http`, Slack `Api`/`Http`/`RateLimited`) either merge them (breaks matches) or route on `kind()` (does not); typed "missing result" variants become `Malformed`.
Shares mechanism with: A1/A4 (refusal → `Error<D>`), F2 (the `items` path is a list-envelope unwrap; `rest` carries Cloudflare `result_info` and MediaWiki's continuation), G (Slack, Notion, Registry wrappers exist because the generated `send()` was not envelope-aware), H (rustdoc names `Response<Pairs>` instead of `<P as SendWith<…>>::Output`, IDEAS seed B4).
Screen:
- C1: "the body is `E<value>`" is a provider fact.
- C2: the split and the refusal rule are Rust (`open`); no field-path language in the declaration (contrast B6).
- C4: `_` is written at the owner and changes meaning only where `envelope` is written; each `->` is the documented value.
- C5: `Value ≠ T`, or `Error` ≠ the declared `error`, is reported at the `envelope` statement.
- C6: one sentence.
- C7: `prepare()?.send()` returns the raw envelope.
- C9: wire test for that escape; test that a 200 refusal and a 4xx give one `Failure<D>` type with the real status; compile-fail for the value mismatch.
- Ranked below B2 on glue per concept (≈60 vs ≈150) and SDKs (6 vs 12), although its cross-family reach (A in-2xx, G wrappers) is larger.

## B4. `status 200, 202;`: the statuses an endpoint succeeds with — KEEP (rank 4)
Sources: AB-claude.md § B4
Fact: this operation answers with exactly these statuses (Snowflake 200/202, Registry 201/202/204, Drive 308, S3 `HeadObject` 200/206/304).
Mechanism: endpoint or scope statement. Absent: any 2xx, as today. Present: only the listed statuses reach the format (its `context` already sees the status); any other status, another 2xx included, is `ErrorKind::Status`, read as `D` under A1; a listed 3xx is an answer. What a 202 *means* stays in the format and SDK.
Declaration: Snowflake (`api.rs:45-51`):
```rust
        POST submit(query("requestId") request_id: RequestId, body statement: Statement) -> StatementBody {
            query { "async" = "true"; }
+           status 200, 202;
            response StatementJson;
        }
// Registry: `PUT put_manifest(..) { ..; status 201; }`, `POST start_upload(..) { ..; status 202; }`
```
Call site: unchanged; an unexpected 2xx is Concord's `Status` error instead of Registry's `ProtocolProblem::UnexpectedStatus`.
Glue removed: ≈60–90 lines in 4 SDKs (lane estimate). Checked: Registry `expect_status` (`error.rs:785-806`, 22 lines) and its 9 call sites (`client.rs:139`, `blob.rs:63,125`, `upload.rs:98,239,285,342`, `manifest.rs:277,368`) exist; Snowflake threads `&[200, 202]` through `client.rs:169,188,224,295,326`.
Counterexample: Elasticsearch GET 404 (`found:false` vs a missing index) cannot be decided by status → keeps its `Get404` decoder. Snowflake keeps `StatementJson`, because 200 and 202 have different bodies. Drive 308 still needs its format to read `Range`.
Simple control: unchanged.
New concepts: `status N, ..;`.  Breaking: none (opt-in).
Shares mechanism with: A1 (unlisted statuses go to the error document), A6 (killed per-status table: S3's 304 lands here as an answer).
Screen:
- C1: a status code is a provider fact.
- C3: absent means today's rule.
- C4: written on the endpoint.
- C5: a bad status is reported at the statement.
- C9: wire tests for an unlisted 2xx and a listed 304.
- Cost: one DSL line per adopting endpoint (M4 rises for Registry). The weakest KEEP; drop it first if the gallery needs room.

## B5. Generated `send()` always returns `concord::Response<T>` — KILL (C3: the simplest call changes shape)
Sources: AB-claude.md § B5
It deletes Nager's `send KeepHead;` but changes the result of every policy-free `send()`: the quickstart's first example and Nager's README gain `.value`, a new user meets `Response` on the first call, and callers of the 16 bare-`T` SDKs (GitHub, Kubernetes, Shopify, Graph, …; lane count) change. Two terminal shapes (`send()` → `Response<T>`, `Prepared::send` → `T`) need a paragraph (C6, in the lane's own words). Its glue (≈20 stateless head-plus-document policies) is reachable opt-in through B1's core `KeepHead`.

## B6. Declared discriminated body contract (`response Json { discriminant "success": bool; true => value "result": _, notes N; false => error E; }`) — KILL (C6: needs a predicate vocabulary)
Sources: AB-codex.md § B3 (and "Rejected: declared field paths" in AB-claude.md)
The lane's own coverage table needs more than a boolean tag: required and forbidden members plus a non-empty code (Slack), an empty/non-empty list partition (Kraken), root-vs-nested presence (QuickBooks). That is the "rules language" its stated kill condition rejects. Field selectors also need a codec document-view capability that JSON gets first (C7 risk). Cloudflare's success-first lazy parse works in B3's Rust `open` (`RawValue`), which also keeps this design's typed refusal-by-value (`Refusal::Provider`).
