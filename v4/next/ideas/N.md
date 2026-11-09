# N. Removal and simplification — merged ideas
Sources: IN-claude.md, IN-codex.md. 9 lane designs → 5 entries: 4 KEEP, 1 KILL, 0 MERGED.

Evidence checked at `ba10189b` in `work/wt/rv-p1` (read-only); "checked" = opened or counted in this merge, "lane estimate" = not re-derived. Per the brief, a removal of something with no users needs only a usage check and is not killed for removing no glue. No SDK note reports a removal candidate (`work/inventory/FRICTION.md` § N).

## N1. Delete what nobody uses — KEEP (rank 1)
Sources: IN-claude.md § N1, § N2; IN-codex.md § N1, § N2, § N3.
Fact: none; surface removals backed by a usage check.
Mechanism: (a) delete the `concord-extras` crate (a 6-line `pub use concord_codecs::*`) after moving its consumers to `concord_codecs`; the `concord` feature named `extras` (renamed `Cargo.toml:13`, amazon-s3) is not the crate and stays. (b) delete `concord_pagination::first_page_then` with `Plan::Waiting` and the discovery-only `P` parameter; keep `Outcome::DiscoveryFailed` (used by `jobs.rs:312`, checked). (c) deprecate, then delete core `Error::meta()`, migrating each call by meaning — `head_meta()` (a head arrived) or `response()` (an unread response is kept) — not by a blind rename. (d) retire the V3 migration diagnostics (`reject_via`/`ViaSite`, in-place formats, parenthesized media types, `advice()`). Usage outcome for the capability crates: used, so not removable; README/CHANGELOG stop calling auth, retry and pagination "pilot" (ratelimit, 4 SDKs, stays one) — a wording decision on their supported contract.
Declaration: unchanged in every SDK. A stale V3 declaration `POST create(item: NewItem via Json)` gets syn's parse error at `via` plus one note pointing at MIGRATION.md, instead of today's executable fix.
Call site: downstream `concord_extras::Json` → `concord_codecs::Json`; `err.meta()` → `err.head_meta()` or `err.response()` by intent.
Glue removed: 0 SDK lines by design. Concord: extras src 6 + Cargo 29 + README 8 lines (+ licences) and two consumer checks (lane ≈ 150 with scripts); `first_page_then` at `independent.rs:107-136` plus `Plan::Waiting` arms (checked) — codex's net ≈ 100–120 library lines — and `tests/first_page.rs` (243 lines, checked); `error.rs:268-276`; V3 diagnostics ≈ 180–220 macro lines (lane) and 501 test lines (UI cases 356 + `applied_advice.rs` 145, checked). Usage checked: no SDK or app manifest names `concord-extras` (users: `checks/consumers/extras-{core,legacy}` and two archived `v4/extensions-real-apis/review/*/probes` manifests outside the workspace); `first_page_then` has no caller outside its crate (Snowflake could not use it, `snowflake-sdk/FRICTION.md:45`); core `Error::meta()` is called only in `checks/` (4 sites) and is **not** deprecated today (`crates/concord-core/src/error.rs:268-276` — corrects IN-claude); `advice()` serves only V3 messages (`syntax.rs:400,547,1644`, `model.rs:2523`); capability crates are in 20 (auth), 16 (retry), 4 (ratelimit), 39 (pagination) SDK manifests.
Counterexample: grep is not type-checked (`Operation::is_endpoint()` once looked unused and Stripe filters with it), so (c) goes through a deprecation build first. `head_meta()` and `meta()` differ on Decode and mid-body failures (`checks/core-concepts/tests/x9_metadata_terminal.rs:70,204` assert `meta()` is `None` there), so IN-claude's `meta()` → `head_meta()` migration is wrong for those cases. A published downstream user of `first_page_then` loses its tested single-budget, first-page-once contract with no equivalent.
Simple control: unchanged (Nager uses the facade with `json`; its own `Error::meta()` is an SDK method, unaffected).
New concepts: −1 crate, −1 traversal entry point (and its engine state), −1 accessor, −V3 spellings.  Breaking: `concord_extras` paths; `first_page_then` and named `Independent<…, P>` types; core `Error::meta()` after deprecation; V3 diagnostic text.
Shares mechanism with: F (first-page-discovered plans, if a real consumer ever fits).
Screen:
- C5: (d) keeps the error at the user's token but loses the executable advice — middle path: keep the single `via` message.
- C6: one crate/facade hop and one ambiguous accessor fewer. C7: `concord_codecs` holds the same items.
- C9: the consumer matrix and dependency-graph rules are edited, not weakened; removed tests go with the removed feature; head/response positive and negative cases stay.

## N2. Header values are text: retire the closed `ToHeaderValue` — KEEP (rank 2)
Sources: IN-claude.md § N4.
Fact: "ntfy reads `X-Priority` as the priority's number" — the header placement is written where the value is declared, like `header("x-title") title?: String` two lines above.
Mechanism: a value placed directly as a header is either a native `HeaderValue` (bytes and flag, as today) or any text (`Display`, or D2's `WireText`), as templates already allow. Dispatch at the expansion site by autoref — a `HeaderValue` probe (also `&`, `Box`, `Rc`, `Arc`, `Cow`) before a text probe — because a blanket `impl<T: Display> ToHeaderValue for T` beside `HeaderValue` is E0119. Every member of today's closed set has a `Display` with the same bytes. Optional, same rule: the `Text` request format encodes any `Display` (removes ntfy's 8-line `PlainText` encoder).
Declaration: ntfy `examples/ntfy-sdk/src/lib.rs:117-129` (checked) →
```rust
    priority?: Priority, tags?: Tags, cache?: Cache,
) -> Event {
    headers { "x-priority" = "{priority}"; "x-tags" = "{tags}"; "x-cache" = "{cache}"; }
// after
    header("x-priority") priority?: Priority, header("x-tags") tags?: Tags, header("x-cache") cache?: Cache,
) -> Event {
```
Call site: unchanged (`set_priority(Priority::High)`).
Glue removed: checked — 41 single-hole `"name" = "{x}";` lines in 9 SDK `src/` trees (amazon-s3 17, stripe 6, paypal 5, github 4, ntfy 3, clickhouse 3, one each in postmark, xero, zendesk; a few may be query blocks), lane 40 in 6 SDKs, plus the `headers { }` block lines around them; `ToHeaderValue` is named by 0 SDK or app files. FRICTION I1 is the most-reported input friction (9 SDKs: "`ToHeaderValue` is sealed"). DSL lines per endpoint fall (a charter measure).
Counterexample: borrowed `HeaderValue` and aliases (renamed `Native`, `each tags?: Vec<…HeaderValue>`) — dispatch by type keeps the native path. Header order between different names is unspecified (AUTHOR_GUIDE §4.7), so moving a template into the parameter list changes nothing tests may rely on. A credential newtype without `Display` needs D2.
Simple control: unchanged (no headers).
New concepts: −1 public trait (`ToHeaderValue` becomes hidden), −1 rule.  Breaking: code naming `concord::ToHeaderValue` (0 sites outside Concord's tests); wire bytes unchanged.
Shares mechanism with: D2 (`WireText` as the one text contract); I1 (`WireDisplay` supplies the text); K/L (declaration lines per endpoint).
Screen:
- C1: each line is a placement fact; the template indirection goes. C4: one rule ("a header value is native or text"). C6: removes a rule and its paragraph.
- C5: risk — plain autoref reports "no method found" for a type with neither form; a checked wrapper with `#[diagnostic::on_unimplemented]` is required.
- C8: same bytes. C9: wire tests per former closed-set type, a test that sensitivity is unchanged, a UI test for a type that is neither.
- Risk: the closed set may be deliberate (a float, or a human-readable `Display`, cannot reach a header by accident); autoref specialization is clever code.

## N3. Operation identity always generated: remove `operations Name;` — KEEP (rank 3)
Sources: IN-codex.md § N4.
Fact: every declared endpoint has an identity in the client's operation tree, whether or not the current policy uses it.
Mechanism: always generate `<Client>Operation` (existing variant and `cfg` rules); delete the opt-in statement; generated `send()` always targets `SendOperation<T, ClientOperation>`, and one core blanket adapter forwards to an existing `SendWith<T>` policy (a non-async forwarding method, no added `Sync`/`Send` bounds). Operation-aware policies keep their impls.
Declaration: Sentry `examples/sentry-sdk/src/lib.rs:41-48` →
```rust
send policy -> Result<Response<_>, Error>;
/// Every Sentry operation, as the send policy classifies it (admission bucket, writes, paging).
operations SentryOperation;          // deleted; `SentryOperation` is still generated
```
Call site: unchanged (`api.organization("acme").issue("42").get().send().await?`).
Glue removed: checked — 8 `operations` statements (github, datadog-events, kraken, sentry, paypal, square, strava, zendesk) and their ceremony comments; Square's pure forwarding `SendOperation` impl (`square-payments-sdk/src/lib.rs:120-133`, 14 lines, discards `_operation`); the macro's dispatch mode (`emit.rs:2112-2124,2441-2449`, early return `:49-53`). 0 classification-table lines.
Counterexample: Square implements both traits — its forwarding impl must be deleted (safe: it only calls `SendWith::send`; `SquareOperation` stays, used by `error.rs:45-51`). A downstream dual impl with different operation-aware behavior must move its agnostic half to a helper (an enumerated break). All 8 current enum names already match `<Client>Operation`.
Simple control: Nager's declaration, `KeepHead` policy and calls unchanged; its rustdoc gains `NagerHolidaysOperation`.
New concepts: none in the DSL (−1 statement); an always-generated enum and a blanket adapter rule.  Breaking: `operations Name;` removed (non-conventional names need a Rust alias); Square's adapter; downstream dual-trait impls; new generated-name collisions.
Shares mechanism with: E (read/write/paged classification reads this enum); O (send-policy traits).
Screen:
- C1: removes a statement that names and switches on generated machinery — a C1 violation in today's DSL.
- C2: classification stays Rust. C3: Nager's declaration is unchanged, but its generated surface gains an enum it never uses (risk).
- C5: sender diagnostics for blanket-adapted policies must name the user's policy, not the adapter.
- C9: simple and operation-aware policies compile; `cfg` variants; exhaustive-match failure on a new endpoint; a dual-impl fixture; request bytes unchanged.
- Risk: a core blanket impl spends downstream coherence freedom to save eight lines and a compiler mode.

## N4. The auth scheme is declared: retire `concord_auth::{bearer, authorization}` — KEEP (rank 4)
Sources: IN-claude.md § N3.
Fact: "Cloudflare authenticates with `Authorization: Bearer <api token>`" (charter fact table: auth scheme and placement).
Mechanism: no new syntax — `headers { AUTHORIZATION = "Bearer {token}"; }` already works with `String` (airtable, dropbox, github, google-drive, notion, salesforce, slack do it, checked); D2's secret hole covers SDK token types. `concord-auth` keeps `TokenCache` (refresh is behavior) and `basic` (a base64 a template cannot express); `bearer` and `authorization` are deprecated, then removed.
Declaration: Cloudflare `examples/cloudflare-zones-sdk/src/lib.rs:61-67` (checked) →
```rust
    /// `Bearer {api_token}`; build it with [`Cloudflare::connect`] or `concord_auth::bearer(token)`.
    #[sensitive] header(AUTHORIZATION) authorization: HeaderValue,
// after
    #[sensitive] api_token: Secret<String>,
) {
    headers { AUTHORIZATION = "Bearer {api_token}"; }
```
Call site: `Cloudflare::connect(http, token)?` → `Cloudflare::new(http, CloudflareConfig { api_token: Secret::new(token), policy: SendPolicy::default() })` (`connect` may stay, minus the credential step); refreshing SDKs: `set_authorization(bearer(&t)?)` → `set_access_token(Secret::new(t))`.
Glue removed: corrected — the two functions are called from 17 `src/` files in 16 SDKs plus Square's re-export (checked), not 20 files. Lane estimate 60–100 lines over 9 static-token SDKs (the credential step and its error mapping; 7 doc lines restating the scheme, glue:docs). Overlaps D2's header-plumbing count; not additive.
Counterexample: Elasticsearch's `authorization("ApiKey", encoded)` becomes `"ApiKey {api_key}"`, but the base64 of `id:key` stays Rust (no deletion). `basic` (11 SDK files) stays. `bearer` refuses spaces and non-ASCII and a template does not: the SDK's token type must check, and a bad token becomes a `Build` error at `prepare()` instead of at construction.
Simple control: unchanged (no auth).
New concepts: none beyond D2's `Secret`.  Breaking: two functions; `Config` fields change from `authorization: HeaderValue` to the token type; public `set_authorization(HeaderValue)` setters (which admitted any scheme) go.
Shares mechanism with: D2 (secret holes); C (credential construction and setters); O (the `concord-auth` capability crate).
Screen:
- C1/C2: the scheme moves into the declaration (today restated in Rust by `bearer()` in 16 SDKs); refresh stays Rust. C3: unchanged.
- C5: validation moves from construction to `prepare()` (risk).
- C9: canaries already cover template-built sensitive headers; add one for `Secret` holes.
- Risk: for rotating credentials the saving is 1–3 lines per site; depends on D2 for secret token types.

## N5. Nager's error stops caching its head — KILL (C3: changes the simple control's public API; removes almost nothing)
Sources: IN-codex.md § N5.
An SDK-local edit, not a Concord removal: drop `meta: Option<Box<ResponseMeta>>` and read `source.head_meta()` on demand — 2 removed lines (`nager-holidays-sdk/src/error.rs:31,71`, checked) — while Nager's public `Error::meta()` changes from `Option<&ResponseMeta>` to an owned `Option<ResponseMeta>`. Error and metadata wrappers belong to families A/B.
