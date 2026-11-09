# I. Values and inputs — merged ideas
Sources: IN-claude.md, IN-codex.md. 10 lane designs → 9 entries: 4 KEEP, 2 KILL, 3 MERGED.

Evidence checked at `ba10189b` in `work/wt/rv-p1` (read-only); "checked" = opened or counted in this merge, "lane estimate" = not re-derived. IDEAS seed 3 (diagnostics-only fix) is not an entry: `#[diagnostic::on_unimplemented]` is already on `IntoValue` (`crates/concord-core/src/value.rs:52-56`, checked) and does not fix inference.

## I1. `#[derive(concord::WireDisplay)]`: a value's wire text from its serde spelling — KEEP (rank 1)
Sources: IN-claude.md § I3.
Fact: the wire spelling of an enum value or identifier — already written once, in its serde attributes.
Mechanism: the derive expands to `impl Display` calling `concord::__private::wire_text(self, f)`, a ~100-line serde `Serializer` that accepts scalars only (str, char, bool, numbers, unit variants, newtype structs, untagged newtype variants); anything else is `fmt::Error`, which Concord already reports as a `Build` error naming the placement (AUTHOR_GUIDE §6). Lives in `concord-codecs` behind `serde` (enabled by `json`/`form`).
Declaration: `api!` unchanged (`query sort?: PullSort`); GitHub `examples/github-sdk/src/params.rs:40-61,87-100` →
```rust
wire_enum!(PullSort { Created = "created", Updated = "updated", Popularity = "popularity", LongRunning = "long-running" });
// after
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, concord::WireDisplay)]
#[serde(rename_all = "kebab-case")]
pub enum PullSort { Created, Updated, Popularity, LongRunning }
```
Call site: unchanged; the same text goes on the wire and `prepare()` URLs are identical.
Glue removed: checked — 89 newtype `Display` impls that only delegate to their field (445 lines) reproduce the lane's count exactly; unit-enum literal matches are 23 (lane) to 42 (a looser heuristic) impls; 11 per-SDK spelling macros exist (`identifier!`, `wire_enum!`, `provider_enum!`, `request_enum!`, `text_id!`, `numeric_id!`, `snowflake!`, `stripe_id!`, `id_type!`, `open_enum!`, `text_value!`). Lane estimate ≈ 400–650 net lines in 32 SDKs; these are candidates (labelled model/other), not all placed in a request. `#[serde(untagged)] Other(String)` also replaces hand-written `From<String>`/`Deserialize` of open enums.
Counterexample: text spelling ≠ serde spelling — S3 `HttpDate` (IMF-fixdate in `If-Modified-Since`), GitHub `Labels` (comma-joined in a query, an array in JSON), Stripe timestamps — keep their hand-written `Display`. Deriving on a sequence fails at `prepare()`, not at compile time.
Simple control: unchanged (Nager's `HolidayType` is response-only).
New concepts: one derive.  Breaking: none (opt-in).
Shares mechanism with: K (how values become request text); G (fewer constructors and accessors); N2 (one text contract for every placement).
Screen:
- C1: no DSL word. C2: the spelling is stated once and read by Concord (today restated by hand). C4: serde's own rules.
- C5: weaker — misuse on a non-scalar is a `prepare()`-time `Build` error, not a compile error at the derive (the lane's own limit).
- C7: a plain `Display`, usable outside Concord. C9: unit tests per serde attribute, plus wire tests.
- Risk: `serde_plain::derive_display_from_serialize!` already does this, so a recipe with zero Concord concepts may remove the same glue; general-purpose derives drift toward a framework.

## I2. Declared numeric bounds: `query per_page?: u8 in 1..=100` — KEEP (rank 2)
Sources: IN-claude.md § I2.
Fact: the provider documents the range of a numeric input ("`per_page` is 1 to 100").
Mechanism: the parser accepts `in A..=B`, `in A..` or `in ..=B` after the type, before `= init`. `__private` checks next to the existing empty-segment refusal: out of range is `ErrorKind::Build` with `Delivery::NotSent`, the message names the key and range, never the value. A literal initializer out of range is a compile error at the literal. Behavior reads `pub const PER_PAGE_RANGE: RangeInclusive<u8>` on the owner type.
Declaration: GitHub `examples/github-sdk/src/api.rs:81` (+ `src/params.rs:176-207` `PerPage` and `ParamError::PerPage`) →
```rust
GET labels(query per_page?: PerPage) -> Vec<Label> { path "labels"; }        // before
GET labels(query per_page?: u8 in 1..=100) -> Vec<Label> { path "labels"; }  // after
// Cloudflare: query per_page?: ZonesPerPage  →  query per_page?: u8 in 5..=50
```
Call site: `repo.issues().set_per_page(PerPage::new(50)?)` → `repo.issues().set_per_page(50)`; an out-of-range value now fails at `prepare()`/`send()`, not at construction.
Glue removed: checked — the 15 bounded newtypes the lane lists sum to 463 non-blank lines (github 28, gitlab 29, cloudflare 41, stripe 26, discord 37, mastodon 28, datadog 27, zendesk 28, intercom 24, europe-pmc 26, arxiv 22, usgs 23, mailgun 22, amazon-s3 48, salesforce 54), ≈ 170 already labelled glue:paging; plus their error variants and the inline range checks in 6 forwarders (airtable, hubspot, linear, google-drive, strava, xero; lane). FRICTION I2: 8 SDKs report undeclarable input constraints.
Counterexample: a bound on a body field (Notion `page_size`, Slack, Dropbox, Shopify GraphQL `first`) — the declaration cannot see inside a body, those newtypes stay (≈ 80 lines). arXiv's 1–100 is an SDK choice narrower than the provider's 2,000 — declare `1..=2000`, keep the narrower rule in Rust. Callers matching `ParamError::PerPage` lose that variant.
Simple control: unchanged (`year: u16` has no documented bound).
New concepts: `in RANGE` on a value; the `<NAME>_RANGE` constant.  Breaking: none until adopted; adopting SDKs change setter types (`PerPage` → `u8`) and lose typed range errors.
Shares mechanism with: F (page-size facts traversal code reads; 4,006 glue:paging lines overall); G (validating forwarders); A (range error variants).
Screen:
- C1: restates the documented range. C2: a check of a declared fact, like the existing "empty, `.` or `..`" refusal; no clamping or retry.
- C4: visible on the line and in the setter doc. C5: a bad literal is a compile error at the literal; a bad run-time value is a `Build` error naming the key, never the text. C8: checked before I/O.
- C9: wire test (refused, `NotSent`, the server sees nothing), compile-fail test, canary for a `#[sensitive]` bounded value.
- Risk: the first step toward a validation language (lengths, patterns — the V2/V3 slope); GitHub clamps rather than rejects, so refusing may be an SDK choice.

## I3. Exact generated inputs: stop converting through `IntoValue` — KEEP (rank 3)
Sources: IN-claude.md § I1, IN-codex.md § I1; variants IN-codex.md § I3 (entry I6), § I4 (entry I7).
Fact: none new — an input declared `date: Date` is a `Date`; the generated signature says exactly what the declaration says.
Mechanism: generated positional arguments, `set_*`, parameter-group setters and initializers take `T` instead of `impl IntoValue<T>` (`crates/concord-macros/src/emit.rs` ≈ :194-204, 779-824, 873-946, 2524). Variants: (a) exact unless written `String`/`HeaderValue` (claude; H03b arm A′, implemented at `fc16090d`; B′ `1329a3e5` names the generic so E0283 names the value); (b) exact everywhere, `IntoValue` kept as a migration utility (codex); (c) bodies only (I7); (d) initializers only (I6).
Declaration: unchanged in every SDK (Frankfurter `src/lib.rs:50`: generated `date: impl IntoValue<Date>` → `date: Date`). One repo migration: renamed `src/lib.rs:30` `accept: Native = &ACCEPT` → `= ACCEPT.clone()` (checked).
Call site: Frankfurter `fx.historical_pair("EUR", "USD", "2024-01-15".parse::<Date>()?)` → `…, "2024-01-15".parse()?)`; Kraken `balances(BalanceOptions::default())` → `balances(Default::default())`; Cloudflare `cf.zone(ZoneId::from(ZONE))` → `cf.zone(ZONE.into())`.
Glue removed: 0 SDK `src/` lines (both lanes; `IntoValue` appears in no `examples/*/src`, checked). Caller side measured in H03b (decision commit `36d37619`, archive branch `dz/h03b`, checked present): witness snippets 134/285 → 187/285 (+18.6 points), 0 regressions, compile time and size ≤ 1.01×. Docs: the E0283 paragraphs (CALLER_GUIDE, DIAGNOSTICS) shrink to one sentence. Closes FRICTION I3 (5 SDKs; Frankfurter `FRICTION.md:16-22`, checked).
Counterexample: borrowed `HeaderValue` through an alias (renamed `Native = &ACCEPT`; `crates/concord/tests/native_headers.rs:155,292`; `formats.rs:372-373`) → `.clone()` (K1, 4 repo sites). Generic `V: IntoValue<u16>` forwarders: none in the 55 SDKs; downstream calls `IntoValue::into_value(v)` (K2). `String`, the most common input, gains nothing under (a).
Simple control: (a) unchanged — `holidays("AT", 2026)` compiles (measured Nager 9/22 → 14/22, PokeAPI 15/36 → 25/36); (b) Nager's call becomes `holidays("AT".into(), 2026)`.
New concepts: none (the "annotate `.parse()`/`.into()`/`Default::default()`" rule goes; (a) adds the lexical `String`/`HeaderValue` exception).  Breaking: K1 borrowed forms into aliases of `String`/`HeaderValue`; K2 generic `impl IntoValue<T>` values; K3 function values through K1/K2; K4 hand-written impls of generated group traits.
Shares mechanism with: H (one caller-guide paragraph fewer); G (forwarders that name option types).
Screen:
- C3: variant (b) fails it (every string literal in Nager gains `.into()`), so (b) is out; (a) holds (measured).
- C4: (a) follows the type as written, so an alias of `String` silently becomes exact — deterministic and visible in the declaration's spelling, but a surprise; codex rejects (a) on this ground. Shown at the gallery with the K1–K4 list, as IDEAS asks.
- C5: a wrong type becomes E0308 naming the user's type; E0283 on `String` still names `IntoValue` unless B′.
- C9: the prototype carries `tests/exact_inputs.rs` (144 lines) and updated UI stderr.

## I4. Initialized bodies: `body options: T = T::default()` — KEEP (rank 4)
Sources: IN-claude.md § I5.
Fact: Kraken accepts a private read with every option absent — a body has a starting value, like any other value.
Mechanism: delete the parse error "a body cannot have a default" (`crates/concord-macros/src/syntax.rs:1607`, checked); an initialized body is stored like an initialized value (not positional, `set_options(v)` replaces it, no `maybe_`). The language loses an exception instead of gaining a word.
Declaration: Kraken `examples/kraken-spot-rest-sdk/src/lib.rs:55` (also :59, :64; checked) →
```rust
POST balances(body options: BalanceOptions, #[sensitive] query account_id?: String) -> Envelope<Balances> {
// after
POST balances(body options: BalanceOptions = BalanceOptions::default(), #[sensitive] query account_id?: String) -> Envelope<Balances> {
```
Call site: `kraken.account().balances(BalanceOptions::default()).send()` → `kraken.account().balances().send()`.
Glue removed: ≈ 10 SDK lines (lane estimate; `account_snapshot` at `lib.rs:136-153` passes two default bodies, checked; the "Construct with `Default`" doc at `models.rs:103`), plus test call sites; closes Kraken `FIRST_USE.md` F5. FRICTION I4 lists 4 SDKs for opaque bodies; only Kraken's defaulting is addressed.
Counterexample: a caller passing the body positionally breaks when the SDK adopts the initializer (`balances(o)` → `balances().set_options(o)`), a per-endpoint SDK choice. Dropbox `list_folder` options have no `Default` (the path is required) and are not a candidate (AUTHOR_GUIDE §2).
Simple control: unchanged.
New concepts: none (one exception fewer).  Breaking: none until an SDK adopts it.
Shares mechanism with: G (forwarders that pass a default body).
Screen:
- C1: "every option is optional" is documented by Kraken. C4: `= expr` reads as it does for values. C6: removes a rule.
- C9: the parse test that expects the error flips; a wire test sends the default body.
- Risk: one witness (Kraken, 3 endpoints), and I3 already makes `balances(Default::default())` compile.

## I5. Secret holes: public `WireText` plus `Secret<T>` — MERGED into D2
Sources: IN-claude.md § I4.
Same mechanism as D2 (a core secret type; text placements bound on a public trait with a blanket `Display` impl). Its compile assertion "a secret type used as text must be marked `#[sensitive]`" is D2's preferred variant (c); its ntfy and Stripe witnesses go to D2, and its 9 SDKs that format `"Bearer {}"` by hand go to N4.

## I6. Exact initializers only — MERGED into I3
Sources: IN-codex.md § I3.
Variant (d) of I3: only generated default functions drop `IntoValue::into_value(expr)` (`emit.rs:791`); calls and setters are unchanged. 0 SDK lines; leaves the Frankfurter date witness untouched and makes initializers differ from setters.

## I7. Exact bodies, converting navigation values — MERGED into I3
Sources: IN-codex.md § I4.
Variant (c) of I3: exact `T` for body arguments and body setters, chosen by the visible `body` placement (no alias rule). 0 SDK lines; does not reach the Frankfurter query `Date`; renamed's `api.note("hi")` becomes `api.note("hi".into())`.

## I8. `concord::Identifier`: identifier newtypes accept borrowed text — KILL (removes almost nothing for a new concept)
Sources: IN-claude.md § I6.
A public marker trait plus `IntoValue<T>` impls for `&str`/`String`/`&T` on identifier newtypes: 0 SDK `src/` lines, ≈ 13 Cloudflare test call sites. It conflicts with I3 for the same types (borrowed acceptance and exact inference cannot both hold), and `"z".into()` and `Default::default()` stay E0283.

## I9. Named input records: `<Endpoint>Input` and `<name>_input(record)` — KILL (C3: the simple case gains generated items; removes nothing)
Sources: IN-codex.md § I2.
A second generated input surface for every endpoint with required arguments (e.g. `NagerHolidaysHolidaysInput` + `holidays_input`): 0 SDK lines (the lane books none), while Nager's generated surface grows by a struct and a method per endpoint. It does not fix setter inference, and FRICTION I6's hand-projected filter records hold optional query values, which these records exclude.
