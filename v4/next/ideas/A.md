# A. Errors — merged ideas
Sources: AB-claude.md, AB-codex.md (family A parts). 8 lane designs → 6 entries: 2 KEEP, 2 KILL, 2 MERGED.

Evidence checked at `work/wt/rv-p1` `ba10189b`: 42 crates call `Received::document::<F, D>` (41 SDKs +
quickstart, as AB-claude says); 31 SDKs keep a `Result<_, concord::Error>` document field next to a head
(matches AB-claude); **12** crates, not 11, read error bodies with `bytes_limited`; 57
`impl From<concord::Error> for` in 52 crates. `work/inventory/FRICTION.md` now exists (both lanes
said it did not): A1 "error document cannot be declared" is reported by 38 SDKs, A3 "failure inside a
2xx" by 11, A4 "parallel hand edits per variant" by 5.

## A1. Declared error document: `error D;` with core `Failure<D>` — KEEP (rank 1)
Sources: AB-claude.md § A2, AB-codex.md § A1, AB-claude.md § A1 (holder type; its library-only stage is A3)
Fact: the non-2xx bodies of this API (or scope) are documents of type `D` in codec `F`; the HTTP status stays authoritative whatever happens to the body.
Mechanism: owner statement `error D [in F];`, nearest owner wins (as `format`). Generated `send()`/`decode()` read a non-2xx body once, bounded, into core `Failure<D>` = `ResponseMeta` + `Document<D>` (`Read | Empty | Unrecognized | TooLong | Broken`) + the bounded bytes; the error type becomes `concord::Error<D>` with `provider()`, `document()`, `status()`, kind still `Status`.
Variants: `error D in F;` with the nearest response format as default codec (claude) vs `error F => D;` with the codec always written (codex) · `concord::Error<D = ()>` (claude) vs a separate `CallError<D> { Concord, Http(HttpFailure<D>) }` (codex) · 64 KiB read bound as a convention (claude) vs a Rust read setting (codex).
Declaration: Backblaze B2 Native (`wire.rs:20-25`):
```rust
pub client B2Wire {
    base "https://api.backblazeb2.com";
    path "b2api/v4";
    format Json;
    limit crate::JSON_BODY_LIMIT;
+   error WireErrorDocument;              // codex form: `error Json => WireErrorDocument;`
    send B2Policy -> Result<(_, ResponseMeta), Error>;
// B2Policy loses its non-2xx branch and provider_error() (wire.rs:76-100). S3: `error ErrorDocument in S3ErrorXml;`
```
Call site (Frankfurter README:96-105):
```rust
- Err(Error::Provider(e)) => match &e.document { Ok(p) => show(&p.message), Err(cause) => show(cause) }
+ Err(Error::Provider(f)) => match f.document() { Some(p) => show(&p.message), None => show(f.body()) }
```
An SDK that keeps its enum routes `provider()` through `From<concord::Error<D>>`, so `Error::Provider(..)` matches stay.
Glue removed: claude ≈980 raw lines measured in 13 witnesses (≈780 holders + ≈200 non-2xx branches), ≈1,650 extrapolated to the 31 holder SDKs; codex ≈60–70 net in Anthropic plus ≈20 in B2. Checked against `anthropic-messages-sdk/src/lib.rs:152-167` and `error.rs:219-323` (≈120 raw lines; `request_id`, `retry_after`, `error_type`, `message` ≈30 lines stay → ≈60–80 net, both lanes agree) and the counts above.
Counterexample: Discord 429 `RateLimitBody` vs `ErrorDocument` (`limiter.rs:375-389`) → one `#[serde(untagged)]` type, no per-status syntax. B2 Native's document status contradicting HTTP (`error.rs:255-275`) → a 6-line accessor over `document()`. Elasticsearch GET 404 (`found:false` vs error) → keeps its decoder. Anthropic SSE: only the opening failure uses `D`. Slack/Kraken 200 refusals → B3 (envelope). Cloudflare `attempts` → 10-line wrapper.
Simple control: unchanged (no `error` → identical generated code; `Status` keeps its unread response). Frankfurter could adopt (+1 line, −76 SDK lines `lib.rs:81-86`, `error.rs:24-93`), but M6 counts the +1: leave it unmigrated in the spike.
New concepts: `error D [in F];`, `Failure<D>`, `Document<D>`, `Error<D>` (or `CallError<D>`), `provider()`/`document()`.  Breaking: none without `error`; adopters lose the unread response on `Status` (`into_response()`), `Prepared`/`Received`/`Endpoint`/`Sends` gain a defaulted `D` (codex: contract parameter `C = Plain`) that generic policy bounds must name, and the SDK provider payload changes from a `Result<D, Error>` field to `Failure<D>`.
Shares mechanism with: B3 (envelope `Refusal::Provider` yields the same `Error<D>`), B4 (an unlisted status is read as `D`), B1 (`Failure` holds `ResponseMeta`), B2 (header accessors on `Failure<D>`), E1 (`Failure::operation()` feeds uncertainty), F3 (followed pages go through the policy, so `D` applies to every page).
Screen:
- C1: "errors are `Problem` documents" is a provider fact.
- C2: declared once; recovery stays in Rust.
- C3: opt-in; controls unchanged.
- C4: claude's default codec and the 64 KiB bound are conventions; prefer codex's explicit codec and a Rust bound (a safety cap is not a provider fact).
- C5: "`Json` cannot decode `Problem`" must be reported at the statement.
- C7: head and bytes stay in `Failure`; `prepare()?.receive()` still returns any status unread.
- C8: one bounded read, no retry.
- C9: wire tests per `Document` variant (oversize, empty HEAD, wrong media type, broken body), canary on `Failure` `Debug`/`Display`, compile-fail for a codec mismatch.

## A2. `#[derive(concord::Error)]`: the SDK keeps its enum, loses the plumbing — KEEP (rank 2)
Sources: AB-claude.md § A5, AB-codex.md § A2
Fact: none about the provider. It removes the restated plumbing around Concord's error and keeps every variant callers `match` (AB-claude counts 283 `Error::Provider` and 107 `Error::Http` matches in SDK tests).
Mechanism: a derive on the SDK enum generates `From` per single-field variant, `Display` (delegated for `#[concord]`/`#[provider]`, `#[display("..")]` otherwise), `std::error::Error` with no `source()`, and accessors that walk the variants (`status()`, `head()`, `operation()`, `delivery()`). Codex also adds `error … as SfError` so generated `send()` returns the SDK enum; that clause is dropped here (it names an SDK type in the declaration, C1, and the existing send template plus `From<concord::Error<D>>` already routes it).
Declaration: unchanged. Kraken `error.rs:164-219` (enum, `status()`, 2 `From`, 6-arm `Display`; 56 lines) becomes:
```rust
#[derive(Debug, concord::Error)] #[non_exhaustive]
pub enum Error {
    #[display("Kraken local refusal: {0:?}")] Local(LocalError),
    #[concord] Concord(concord::Error),
    #[provider] Http(Box<concord::Failure<ErrorDocument>>),
    #[display("Kraken successful envelope is missing its result")] MissingResult(Box<ResponseMeta>),
}
```
Call site: unchanged (`Err(Error::Http(f))`, `e.status()`, `e.to_string()`). With A1, Salesforce also drops `SfError::from_concord(error).await` (`error.rs:49-58`).
Glue removed: claude ≈400 raw lines measured in 9 SDKs, ≈1,500–2,000 extrapolated to the 45 `error.rs` (the lane calls this its least certain number); codex Salesforce 65–80 net, 30–45 if the exact `Display` text is kept. Checked: Kraken `error.rs:164-219` (`status()` 12, `From` 10, `Display` 14 lines of plumbing) and Salesforce `error.rs:102-180` (7 `From` impls, `Display`, `Debug`, `Error`) match.
Counterexample: Stripe's conditional `Api` `Display` (`send.rs:398-421`) stays by hand (`#[display(with = fmt_api)]`). Slack `Api` vs `RateLimited` and Cloudflare `attempts` are classification, not derivable. Two `#[provider]` variants or duplicate `#[from]` sources are errors at the attribute, never guessed.
Simple control: declaration unchanged; Nager's `error.rs` 86 → ≈50 lines if it adopts (`kind()`, its HTTP 400 rule, stays).
New concepts: derive `concord::Error`; `#[concord]`, `#[provider]`, `#[display]`, `#[from]`/`#[from(skip)]`.  Breaking: none (opt-in, enum and variants kept); default `Display` text changes unless kept by hand.
Shares mechanism with: D (withholding in derived output, same style as a `concord::Debug` derive), A1 (`#[provider]` payload is `Failure<D>`).
Screen:
- C1/C2: not a declaration feature once `as SfError` is dropped.
- C5: derive errors point at the variant attribute.
- C6: the risk; it must stay "`thiserror` plus Concord accessors" with at most four attributes.
- C9: canary over every variant of a derived enum and its `source()` chain.
- Gallery: show the lanes' cheaper alternative beside it, `thiserror` plus three free functions (`concord::status_of(&e)`, …) and no new surface.

## A3. Library-only `Failure<D>` (`Received::failure::<F, D>(limit)`) — MERGED into A1
Sources: AB-claude.md § A1
The core holder without the statement: each policy still writes `received.failure::<Json, ErrorDocument>(LIMIT)`, so the fact stays restated in Rust (the lane's own "C2: only in part"). It is A1's first implementation stage (≈780 of A1's lines, zero DSL), not a separate gallery entry.

## A4. Errors inside a 2xx join the same error type — MERGED into B3
Sources: AB-claude.md § A3 (and the `false => error` branch of AB-codex.md § B3, killed as B6)
The mechanism is the success envelope: `Envelope::open` returns `Refusal::Provider(d)`, tied to the declared `error D`, so Slack `ok:false`, Kraken's `error` list, MediaWiki's `error` and a 4xx all yield one `concord::Error<D>` with `ErrorKind::Refused` and the real status (FRICTION A3, 11 SDKs). Its glue (≈130 net in 5 SDKs) and its gap (ClickHouse's header-signalled in-stream exception) are recorded in B3.

## A5. Error document carried by the format: `format Problems<Json, ErrorDocument>;` — KILL (C5: a wrong `D` fails silently at run time)
Sources: AB-claude.md § A4
The document is stored erased and read with `err.document::<D>()`: a wrong `D` is a `None` at the access, not an error where it was written, and `send()`'s type no longer says what can fail. It also ties the error document to the success format, false for multi-format SDKs: Registry (`api.rs:67-83`: `Binary`, stream, `Json`) and S3 (`ListXml`, `HeadFormat`, `Download`, `PutFormat`) would need one wrapper per format statement. A1 with an explicit codec covers the same SDKs.

## A6. Per-status failure table (`errors S3Status { 304 => NotModified; otherwise => Api(..) via ErrorXml; }`, `error body absent;`) — KILL (glue: ≈40–50 net lines in one SDK for six concepts)
Sources: AB-codex.md § A3 (IDEAS seed A2; listed as rejected in AB-claude.md "How the designs compose")
Saves ≈40–50 net in S3 (`client.rs:385-407`, `error.rs:263-304`) for six concepts: the table, `otherwise`, `error body absent`, a generated status sum, `CaseError<H>`, a document-state enum. The lane itself says most witnesses use one document for every failure status. Discord's two shapes fit A1 with `#[serde(untagged)]`; HEAD's missing body is an HTTP rule A1's `Document::Empty` already covers, so `error body absent` restates what Concord knows; S3's 304 is a success-status fact (B4).
