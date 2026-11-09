# D. Secrets inside SDK types — merged ideas
Sources: DJM-claude.md, DJM-codex.md (+ IN-claude.md § I4, a family-I design with the same mechanism as D2). 8 lane designs (+1 from I) → 6 entries: 3 KEEP, 2 KILL, 1 MERGED.

Evidence checked at `ba10189b` in `work/wt/rv-p1` (read-only); "checked" = opened or counted in this merge, "lane estimate" = not re-derived.

## D1. Selective `Debug` derive: `#[derive(concord::Debug)]` with `#[sensitive]` fields — KEEP (rank 1)
Sources: DJM-claude.md § D1, DJM-codex.md § D1; variant DJM-claude.md § D3 (entry D4).
Fact: which fields of a provider record are private (Twilio: account SID, from, to, body, error message) and which are public diagnostics (message SID, status, error code).
Mechanism: a derive generating an ordinary `std::fmt::Debug`. Unmarked fields print as std's derive; a `#[sensitive]` field prints `<redacted>` (`Option` shows `None` / `Some(<redacted>)`) and its own `Debug` is never called, so native `HeaderValue`s, readers and closures need none; `#[sensitive]` on the type or a variant prints `Name { .. }`. Nothing is inferred from names or from use in an `api!` block. Variants: field denylist (both lanes, headline) | type-level withholding with a `#[public]` allowlist (D4).
Declaration: Twilio `examples/twilio-sdk/src/models.rs:133-165` (struct + 13-line hand `impl fmt::Debug` with `finish_non_exhaustive`) →
```rust
#[derive(Clone, PartialEq, Eq, Deserialize, concord::Debug)]
pub struct Message {
    pub sid: String,
    #[sensitive] pub account_sid: String,
    pub status: MessageStatus,
    #[sensitive] pub body: Option<String>,   // `from`, `to`, `error_message` marked the same way
    pub error_code: Option<i64>,
}
```
Call site: unchanged (`println!("{message:?}")`); the text becomes `Message { sid: "SM1", account_sid: <redacted>, status: Queued, body: Some(<redacted>), … }`.
Glue removed: Debug impls — charter 361; checked 373 `impl … Debug for` lines in 49 SDK `src/` trees (includes macro templates). Category glue:redaction = 1,478 lines (checked against `work/glue-dev/GLUE.draft.md`), the realistic ceiling. Witness ranges checked: twilio `models.rs:157-165`, registry `model.rs:36-47`, s3 `sigv4.rs:119-130`. Lanes differ: claude ≈ −1,250 of 2,954 Debug lines (80 opaque + 127 partial impls → ≈400 attribute lines); codex books witness deletions only (≈4–12 net lines per impl). FRICTION D1: 18 SDKs report "a derived `Debug` compiles and leaks". Also removes Debug code the classifier files under A (`error.rs`), B (`Response<T>`), C (wrapper clients), F (cursors).
Counterexample: Twilio `Message` — the public fields are simply unmarked. Europe PMC `Source`/`PublicationId` become std derive, ending today's contradiction with `Prepared` (`europe-pmc-sdk/src/input.rs:41-45` redacts what `tests/canary.rs:1-7` says is public; checked). ClickHouse keeps its auth user sensitive (`clickhouse-sdk/src/wire.rs:26-29`, checked) because nothing is inferred. Native values: never formatted. Not handled: Shopify `Var<String>`'s three-way `Absent/Null/Value(<redacted>)` (`shopify-sdk/src/ops/catalog.rs:119-134`) stays hand-written.
Simple control: unchanged (Nager, PokeAPI, Frankfurter have no withholding `Debug`).
New concepts: derive `concord::Debug` (helper attribute `#[sensitive]`, already a Concord word).  Breaking: none in APIs; the `Debug` text of adopting types changes (docs/tests quoting it).
Shares mechanism with: A (error `Debug`), B (`Response<T>` `Debug`), C (wrapper clients), F (cursor `Debug`); J (what may safely be logged); M1 (the canary checks it).
Screen:
- C1: `#[sensitive]` is the existing provider-fact word; no word configures Concord.
- C2/C3: no behavior; no declaration changes.
- C4: every withheld field is marked on its own line; no name or type inference.
- C6: four one-line rules.
- C9: Concord tests a derive corpus with the canary (no marked value in `{:?}`/`{:#?}`; unmarked output equals std's derive; a poisoned field `Debug` is never called).
- Risk: a denylist fails open for a field the provider adds later — the reason authors hand-write `finish_non_exhaustive()` today (D4 is the alternative default).

## D2. A secret value type: `concord::Secret<T>` with a public text-placement trait — KEEP (rank 2)
Sources: DJM-claude.md § D2 (`Sensitive<T>` + `WireText`), DJM-codex.md § D2 (`Secret<T>` storage), IN-claude.md § I4 (secret holes; entry I5 in I.md).
Fact: this value is a credential or opaque token wherever it is stored or placed (ntfy sends `Authorization: Bearer <access token>` and the token is secret).
Mechanism: core `Secret<T>`: constant `Debug` without `T: Debug`, no `Display`/`Deref`, `expose()`/`into_inner()`, conditional `Clone`/`Eq`. Text placements (path, query, template hole) are bound on a public `WireText` trait instead of the hidden `__private::Text: Display` (`crates/concord-core/src/__private.rs:47-56`, checked), with a blanket impl for every `Display` type so current code compiles; `Secret<T: WireText>` writes the inner text; `ToHeaderValue` gains `Secret<HeaderValue>`, flagged sensitive. Variants: (a) storage only (codex); (b) placement, and the type's const decides withholding without `#[sensitive]` (claude); (c) placement, plus a compile assertion that a `Secret` value placed as text is marked `#[sensitive]` (IN-claude I4; claude's own fallback).
Declaration: ntfy `examples/ntfy-sdk/src/lib.rs:103-106`, `model.rs:95-108` (checked) →
```rust
// before: #[sensitive] header(AUTHORIZATION) authorization?: HeaderValue,
//   + AccessToken::header_value(): format!("Bearer {}", ..) + set_sensitive(true); + impl Debug
pub client Ntfy(#[sensitive] token?: Secret<AccessToken>) {     // variant (c)
    base "https://ntfy.sh";
    headers { AUTHORIZATION = "Bearer {token}"; }
    …
}
```
Call site: `Ntfy::new(http).with_token(&AccessToken::new(t)?)` → `Ntfy::new(http).set_token(AccessToken::new(t)?)` (`new` returns `Secret<AccessToken>`); reading is `token.expose()`; `format!("{token}")` no longer compiles.
Glue removed: lane estimate ≈ 450–550 lines (most of the 80 opaque `X(<redacted>)` impls ≈ 400 + header-value plumbing ≈ 150); witnesses checked (ntfy `model.rs:95-108`, s3 `sigv4.rs:39-44,119-130`); 34 SDK/app `src/` trees call `set_sensitive` or `concord_auth::` (checked; lane said 33). Overlaps D1's deletions — not additive. Closes FRICTION D3 (a templated secret needs a leaking `Display`: kubernetes, ntfy, stripe) and D6 (marker repeated per copy, kubernetes) for variant (b) only.
Counterexample: native values — `Secret<HeaderValue>` is placed whole and flagged, never as text. Twilio `Message` stays on D1 (wrapping public model fields adds `.expose()` at every read). S3 `Credentials` wraps the signing secret and session token and keeps `access_key_id` public. Europe PMC ids and public ClickHouse identifiers stay unwrapped; nothing is decided by spelling.
Simple control: unchanged (no secrets).
New concepts: `concord::Secret<T>`, public trait `WireText` (replaces the hidden `Text`).  Breaking: none for current declarations (blanket impl); `concord_auth::{basic, bearer, authorization}` may return `Secret<HeaderValue>` (mechanical); SDK-internal borrows of secrets move to `expose()`.
Shares mechanism with: I (IN-claude I4 = I5, merged here); N4 (retiring `bearer` needs secret holes); N2 (`WireText` as the one text contract, headers included); C (credential construction); F (opaque cursors such as S3 `ContinuationToken`).
Screen:
- C1: the type states the fact.
- C2: no behavior; `expose()` is the caller's decision.
- C4: variant (b) hides secrecy behind aliases (`q: Soql` with `type Soql = Sensitive<…>`, as claude proposes for Salesforce), so the declaration stops showing what is secret (Auditor promise) — prefer (c), where `#[sensitive]` stays visible and the compiler checks it.
- C5: `format!("{token}")` fails with an `on_unimplemented` note naming `expose()`.
- C7: `expose()`, `into_inner()` and native `HeaderValue` remain.
- C9: oracle axis — withheld in every Concord rendering and in `{:?}`, present on the wire; a poisoned inner `Debug` is never called; compile-fail test for an unmarked `Secret` hole.

## D3. `concord::Link`: a URL the provider hands back prints its origin only — KEEP (rank 3)
Sources: DJM-claude.md § D4.
Fact: a URL returned by the provider (`Link: rel="next"`, a signed download `Location`, Shopify's bulk `url`, Salesforce `nextRecordsUrl`) is a bearer capability or opaque cursor: its origin is public, its path and query are not.
Mechanism: core type around `Url`: `Debug` prints the origin only, no `Display`, `origin()`, `expose() -> &Url`, serde `Deserialize` (feature `json`) and `Link::from_header`. `Prepared::follow` accepts `&Link` through a sealed target (same-origin rule unchanged, `crates/concord-core/src/prepared.rs:62-77`).
Declaration: Shopify `examples/shopify-sdk/src/scalars.rs:214-260` (checked) →
```rust
// before: pub struct SignedUrl(Url); + parse() + host() + expose() + Debug (host only) + Deserialize
pub type SignedUrl = concord::Link;   // after; model fields unchanged
```
Call site: `prepared.follow(next.url().as_str())?` → `prepared.follow(next.url())?`; reading the URL stays explicit (`expose()`).
Glue removed: lane estimate ≈ 120–150 lines in ≈ 7 SDKs; witness ranges checked (shopify `scalars.rs:214-260` ≈ 40, github `paging.rs:273-280` and `download.rs:80-89`, registry `pages.rs:39-58`, salesforce `model.rs:28-41`); Microsoft Graph `links.rs` and DigitalOcean `pagination.rs` hold similar types (not counted).
Counterexample: GitHub `ArtifactLocation` must show the download host — the origin is what `Debug` shows. Registry must refuse a cross-origin `Location` — `follow` already does. PokeAPI keeps `next: Option<String>` (a public URL; nothing forces the type).
Simple control: unchanged.
New concepts: `concord::Link`.  Breaking: none (`follow` accepts `&str` or `&Link`).
Shares mechanism with: F (a declared next-link location may make SDK link types unnecessary); J1/J3 (logs print the origin only); C (GitHub's download-host checks).
Screen:
- C1: "server-issued URL" is the provider fact. C2: following and paging stay in Rust (`concord-pagination`). C3: unchanged.
- C7: `expose()`.
- C9: canary in a link's path and query — absent from `Debug` and logs, present on the wire after `follow`.
- Risk: family F's cursor design may absorb the paging half, leaving `Link` for signed downloads only.

## D4. Withheld unless `#[public]`: type-level default for the D1 derive — MERGED into D1
Sources: DJM-claude.md § D3.
Same derive as D1 with the opposite default for marked types: `#[sensitive]` on the struct withholds every field except those marked `#[public]`, in today's dominant `finish_non_exhaustive()` style (Twilio's text stays byte-identical; a field Salesforce adds next year stays withheld). It is D1 plus one word and answers D1's fail-open risk; shown as D1's variant at the gallery. Costs: two defaults to read (C6), and `#[public]` next to Rust's `pub` invites "pub field = printed" misreadings.

## D5. `derive(concord::Diagnostic)`: one literal label per error variant — KILL (removes almost nothing for a new concept)
Sources: DJM-codex.md § D3.
Xero `examples/xero-accounting-sdk/src/error.rs:166-190` (25 lines: a `Display` match and a `Debug` impl, checked) becomes 12 label attributes plus a 5-line delegate: ≈ 7 net lines, one SDK. It is a general Rust error-display derive (thiserror/displaydoc territory), not a fact only Concord knows; error formatting belongs to family A.

## D6. Sensitivity flowing from the declaration into SDK types (IDEAS seed 2) — KILL (C5: generated impls collide with the author's code)
Sources: DJM-claude.md § "D, rejected seed".
The macro would have to emit `Debug` for types it does not own because they appear as `#[sensitive]` values: it conflicts with the author's derive, duplicates across two declarations and is impossible for `String`/`HeaderValue`; and 163 of ≈ 226 withholding impls are on types no declaration names. D2 takes the implementable direction (the type states the fact, the declaration inherits it).
