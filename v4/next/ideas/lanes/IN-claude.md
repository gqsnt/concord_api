# Families I (values and inputs) and N (removal and simplification): claude lane

Base: `work/wt/rv-p1` at `ba10189b`. Read: `CHARTER.md`; `IDEAS.md` (I, N); `GLUE.draft.md`;
`CONSTRUCTION_PROBLEM.md`; `docs/CHEATSHEET.md`; `docs/AUTHOR_GUIDE.md` §2, §4.8, §5.14, §6, §7;
`docs/CALLER_GUIDE.md` §0; `docs/DIAGNOSTICS.md`; `crates/concord-core/src/{value,header,error,__private}.rs`;
`crates/concord-macros/src/{emit,syntax,model}.rs` (input emission, parameter parsing, removed
spellings). I also read the witness and control SDK sources with their `FRICTION.md`, `FIRST_USE.md`
and `INTEGRATION.md`; the de-duplicated catalogue `work/inventory/FRICTION.md` does not exist yet.
Archive records: the H03/H03b card, measurement and prototype (`concord-desirability`, `dz/h03b`)
and the removal table in `v4/sdk50/cross50/PROPOSALS.md`.

Evidence labels used below:
- **observed**: grep, or the glue classifier re-run per line at `ba10189b`.
- **measured**: the archived H03b numbers.
- **inferred**: my estimate.

Nothing was compiled against the repository. One coherence probe (I4, I6) was compiled with rustc
1.99.0 in scratch: the `IntoValue`/`Sealed` shape plus a downstream crate. It shows that a
downstream impl of a Concord trait for a type without `Display` is accepted beside a blanket
`impl<T: Display>`. It also shows that `impl<T: Marker> IntoValue<T> for &str` and
`… for &T` coexist with today's impls.

## Where family I's glue actually is

The classifier has no "input" category, so input glue is counted under other labels (observed):

| Shape | Size | Classifier label today |
|---|---|---|
| Bounded-integer newtypes (`PerPage`, `ZonesPerPage`, `PageSize`, …: struct, checked `new`, `get`, `MAX`, `Display`) | 25 types, 606 non-blank lines, 21 SDKs | 215 glue:paging, 188 model, 160 behavior, 42 other |
| `Display` impls whose only job is the wire spelling (a newtype delegating to its field; a unit enum matched to literals) | ≈112 impls, ≈675 lines, 32 SDKs, plus 10 per-SDK macros reinventing it (`identifier!`, `wire_enum!`, `provider_enum!`, `request_enum!`, `text_id!`, `numeric_id!`, `snowflake!`, `stripe_id!`, `id_type!`, `open_enum!`) | model / other |
| Credentials pre-built as `HeaderValue` in Rust, because a template hole needs `Display` and a secret type must not have one (ntfy `FRICTION.md:113-121`, Stripe `FRICTION.md:155-164`) | 22 SDK `src` files call `concord_auth::{bearer,basic,authorization}`; 9 more format `"Bearer {}"` by hand | behavior / construction |
| Single-value headers routed through a `"{x}"` template because `ToHeaderValue` is closed | 40 declaration lines in 6 SDKs | declaration |
| Caller spellings that do not compile (`Default::default()`, `.into()`, `.parse()`) | H03b corpus: 151 of 285 witness snippets fail at baseline (measured) | not SDK source |

Summary of the designs (SDK lines are inferred from the observed ranges below):

| Design | Mechanism | SDK lines removed | Families touched | Breaks |
|---|---|---|---|---|
| I1 Exact unless text (**smaller than you think**) | generated inputs take `T`; only written `String`/`HeaderValue` convert | ≈0 in `src`; caller side +53 working witness snippets (measured) | I, N, H | K1–K4 (4 repo sites) |
| I2 Declared bounds | `query per_page?: u8 in 1..=100` | ≈460 (15 SDKs) | I, F, G, A | none until adopted |
| I3 One wire spelling | `#[derive(concord::WireDisplay)]` = `Display` from serde | ≈400–650 (32 SDKs, opt-in) | I, K, G | none |
| I4 Secret holes | public `concord::WireText` for holes + `concord::Secret<T>` | ≈60–120 (≈10 SDKs) | I, D, C | none |
| I5 Initialized bodies | `body options: T = T::default()` | ≈10 (Kraken) | I, G | none until adopted |
| I6 Identifiers accept borrowed text | `impl concord::Identifier for ZoneId {}` | ≈0 in `src`; ≈13 Cloudflare call sites | I | none (conflicts with I1) |
| N1 Delete what nobody uses (**smaller than you think**) | `concord-extras`, `first_page_then`, `Error::meta()`; drop "pilot" from used crates | 0 SDK; ≈500 Concord lines | N | 3 items |
| N2 Retire V3 migration diagnostics | macro stops parsing removed spellings | 0 SDK; ≈200 macro + ≈500 test lines | N | diagnostics only |
| N3 The auth scheme is declared | retire `concord_auth::{bearer, authorization}` | ≈60–100 (9 SDKs) | N, C, D | 2 functions, 20 files |
| N4 Header values are text | header placement = native `HeaderValue` or any `Display` | 40 declaration lines (6 SDKs) + 8 (ntfy) | N, L, K | `ToHeaderValue` hidden |

---

# Family I: values and inputs

## I1. Exact unless text (smaller than you think)

**Fact it expresses:** none is new. A value declared `date: Date` is a `Date`. The generated
signature now says exactly what the declaration says; only inputs written `String` or
`HeaderValue` keep their borrowed forms.

**Declaration (author writes):** unchanged in every SDK. The change is in what the macro emits
for positional values and bodies, `set_*`, parameter-group setters and initializers. These are
`emit.rs:199,202` (group traits), `:791` (initializers), `:813,823` (positional), `:887,928,945`
(setters) and `:2524`. The type stays exactly `T` unless it is written with a final path segment
`String` or `HeaderValue`. Frankfurter `src/lib.rs:50`:

```rust
GET historical_pair(base: String, quote: String, query date: Date, query providers?: String) -> Rate { … }
// generated, before
fn historical_pair(&self, base: impl IntoValue<String>, quote: impl IntoValue<String>, date: impl IntoValue<Date>)
// generated, after
fn historical_pair(&self, base: impl IntoValue<String>, quote: impl IntoValue<String>, date: Date)
```

It is already implemented and measured as H03b arm A′ (`fc16090d` on `dz/h03b`: `emit.rs`
+100 lines, `value.rs` ±49). An optional refinement is arm B′ (`1329a3e5`): the remaining
`String`/`HeaderValue` inputs use a named generic, so their E0283 names the value
(`CountryCode`) instead of `concord_core::IntoValue`.

**Call site (caller writes):**

```rust
// Frankfurter (FRICTION.md:16-22: E0283 citing IntoValue<Date>)
fx.historical_pair("EUR", "USD", "2024-01-15".parse::<Date>()?)  →  fx.historical_pair("EUR", "USD", "2024-01-15".parse()?)
// Kraken (FIRST_USE.md F5: "the option type must be imported and named")
kraken.account().balances(BalanceOptions::default())             →  kraken.account().balances(Default::default())
// Cloudflare (FRICTION.md F3; tests/workflows.rs:134)
cf.zone(ZoneId::from(ZONE))                                        →  cf.zone(ZONE.into())
```

Measured (H03b): witnesses 134/285 → 187/285 (+18.6 points), 0 regressions, 0 snippets that
compile with a different request, compile time and expanded size ≤ 1.01×.

**SDK code that disappears:**
- SDK `src/`: about 0 lines. The gain is at call sites.
- Concord docs: the E0283 paragraph at `docs/CALLER_GUIDE.md:83-89`, `docs/DIAGNOSTICS.md:152-157`
  and the `compile_fail` doctest at `value.rs:33-51` shrink to one sentence ("a `String` input
  also takes `&str`/`&String`").
- Closed friction: `frankfurter-sdk/FRICTION.md:16-22`, the import half of
  `kraken-spot-rest-sdk/FIRST_USE.md` F5, and the literal half of
  `cloudflare-zones-sdk/FRICTION.md:48-62`.

**Hardest counterexample:**
- **Borrowed `HeaderValue` through an alias.** `examples/renamed/src/lib.rs:30` declares
  `header("accept") accept: Native = &ACCEPT`, with `type Native = reqwest013::header::HeaderValue`.
  `Native` is not lexically `HeaderValue`, so the initializer becomes exact and `&ACCEPT` is E0308.
  Migration: `= ACCEPT.clone()` (free for `from_static`).
- **The same class (K1) elsewhere:** `crates/concord/tests/native_headers.rs:155` and `:292`
  (alias `Hv`), and `crates/concord/tests/formats.rs:372-373` (`json_body("x")` into
  `type Raw = String`).
- **Generic `V: IntoValue<u16>` forwarders (K2):** there are none in the 55 SDKs (`IntoValue`
  appears in no `examples/*/src`). Downstream code gets E0308. Migration: call
  `IntoValue::into_value(v)` at the call.

**Simple control (Nager):** unchanged. The declaration and `holidays("AT", 2026)` both stay as
they are: `String` keeps `&str`, and `2026` infers `u16`. Measured: Nager 9/22 → 14/22 and PokeAPI
15/36 → 25/36, with no caller change. Frankfurter's declaration is also unchanged.

**Charter check:**
- C1: no new word.
- C3: holds (measured on Nager and PokeAPI).
- C4: the borrowed-text rule follows the type *as written*, so an alias silently changes what is
  accepted. This is the design's one C4 cost.
- C5: a wrong type becomes E0308 "expected `PerPage`, found integer", naming the user's type.
  The remaining E0283 on `String` still names `IntoValue` unless B′ is taken.
- C6: deletes the caller paragraph on target-typed spellings.
- C9: the prototype has `tests/exact_inputs.rs` (144 lines) and updated UI stderr
  (`conversions_are_closed`, `newtype_is_not_converted`).

**New concepts:** none. The rule "annotate `Default::default()`/`.into()`/`.parse()`" goes away
(−1), and the lexical `String`/`HeaderValue` qualifier is added (+1).

**Breaking changes:**
- K1: borrowed forms into aliases of `String`/`HeaderValue` (the 4 repo sites above).
- K2: generic or opaque `impl IntoValue<T>` values passed to inputs that become exact.
- K3: function values instantiated through K1 or K2.
- K4: hand-written impls of generated parameter-group traits whose setter becomes exact.

**Why it might be wrong:** it was measured below its own frozen threshold (+18.6 against +20) and
moves no SDK line. `String`, the most common input type, gains nothing, and a lexical rule about
aliases is the kind of surprise C4 forbids.

## I2. Declared bounds

**Fact it expresses:** "`per_page` is 1 to 100", the provider's documented range of a numeric
input.

**Declaration (author writes):** GitHub `src/api.rs:81,109`, with `src/params.rs:176-207`
(`PerPage`) and `:17-18,29` (`ParamError::PerPage`):

```rust
GET labels(query per_page?: PerPage) -> Vec<Label> { path "labels"; }
→
GET labels(query per_page?: u8 in 1..=100) -> Vec<Label> { path "labels"; }
```

Cloudflare `src/lib.rs:96,127` (with `models.rs:245-294`):

```rust
query per_page?: ZonesPerPage,    →  query per_page?: u8 in 5..=50,
query per_page?: RecordsPerPage,  →  query per_page?: u32 in 1..=5_000_000,
```

Mechanism:
- The parser accepts `in A..=B`, `in A..` or `in ..=B` after the type and before `= init`, in
  `parse_params` beside the initializer.
- `__private` gains one check next to the existing empty-segment refusal (`__private.rs:419-425`).
  A value out of range is `ErrorKind::Build` with `Delivery::NotSent` and the message
  "`per_page` is outside 1..=100". The value itself is never printed.
- A literal initializer outside the range is a compile error (a `const` assertion at the literal).
- Typed access for behavior: the owner type gets `pub const PER_PAGE_RANGE: RangeInclusive<u8>`.
  Paging code that used `PerPage::MAX` reads `*GitHubRepoIssues::PER_PAGE_RANGE.end()`.
- The generated setter doc states the range.

**Call site (caller writes):**

```rust
repo.issues().set_per_page(PerPage::new(50)?)          →  repo.issues().set_per_page(50)
zones.list().set_per_page(ZonesPerPage::new(5).unwrap()) →  zones.list().set_per_page(5)   // cloudflare tests/workflows.rs:47
```

An out-of-range value now fails at `prepare()`/`send()` as `Build`, not at construction.

**SDK code that disappears:** the bounded types placed as query or header values, which the
declaration can state (observed ranges, non-blank lines):

| SDK | Lines |
|---|---|
| github | `params.rs:176-207` (28) + `ParamError::PerPage` |
| gitlab | `params.rs:333-365` (29) |
| cloudflare | `models.rs:245-294` (41) |
| stripe | `types.rs:113-141` (26) |
| discord | `model.rs:285-326` (37) |
| mastodon | `model.rs:265-296` (28) |
| datadog | `query.rs:81-110` (27) |
| zendesk | `models.rs:391-422` (28) |
| intercom | `models.rs:7-33` (24) |
| europe-pmc | `input.rs:117-146` (26) |
| arxiv | `input.rs:70-91` (22) |
| usgs | `input.rs:85-111` (23) |
| mailgun | `events.rs:66-87` (22) |
| amazon-s3 | `types.rs:780-832` (48) |
| salesforce | `ids.rs:591-651` (54) |

That is ≈460 lines in 15 SDKs, about 170 of which the classifier already counts as glue:paging.
Their error variants and `Display` arms go too. So do the inline range checks in forwarders,
where the value is a declared query value: airtable `lib.rs:330`, hubspot `lib.rs:602`,
linear `lib.rs:298`, google-drive `client.rs:296`, strava `lib.rs:222` and xero `lib.rs:218`.

**Hardest counterexample:**
- **A bound on a body field.** Notion `params.rs:6-30` (`page_size` lives in the JSON body,
  `params.rs:72`), Slack `lib.rs:42-62`, Dropbox `models.rs:7-26` and Shopify `ops/mod.rs:19-44`
  (GraphQL `first`). The declaration cannot see inside a body, so these newtypes stay (≈80 lines).
- **An SDK bound narrower than the provider's.** arXiv `input.rs:70` says "a deliberately small
  page of 1–100 entries (provider slice ceiling: 2,000)". Declaring `1..=100` would state an SDK
  choice as a provider fact (C1). The fact is `1..=2000`, and the SDK keeps its own narrower
  rule in Rust.
- **Typed errors.** Callers matching `ParamError::PerPage` lose that variant to a Concord `Build`
  error.

**Simple control (Nager):** unchanged (`year: u16` has no documented bound). PokeAPI and
Frankfurter are unchanged too.

**Charter check:**
- C1: `in 1..=100` restates what the provider documents.
- C2: it is a check of a declared fact, like the existing "is empty, `.` or `..`" refusal. There
  is no clamping, retry or other behavior.
- C3: holds.
- C4: visible in the declaration and in the setter doc.
- C5: a bad literal initializer is a compile error at the literal. A bad runtime value is a
  `Build` error naming the endpoint and the value, never its text.
- C6: one line of docs.
- C8: deterministic, and checked before I/O.
- C9: needs a wire test (refused, `NotSent`, the test server sees nothing), a compile-fail test,
  and a canary for a `#[sensitive]` bounded value.

**New concepts:** `in RANGE` on a value, and the `{VALUE}_RANGE` associated constant.

**Breaking changes:** none for existing declarations. SDKs that adopt it change setter types
(`PerPage` → `u8`) and lose their typed range errors.

**Why it might be wrong:** it is the first step toward a validation language (lengths, patterns),
which is the V2/V3 slope. Also, refusing what GitHub would silently clamp is arguably an SDK
choice, not a provider fact.

**Several families:** F (page-size facts that traversal code can read; 215 glue:paging lines),
G (validating forwarders) and A (range error variants).

## I3. One wire spelling (`#[derive(concord::WireDisplay)]`)

**Fact it expresses:** the wire spelling of an enum value or identifier, written once, in its
serde attributes.

**Declaration (author writes):** the `api!` block is unchanged (`query sort?: PullSort`). The
model changes.

GitHub `src/params.rs:40-61` (`wire_enum!`) and its invocation at `:87-100`:

```rust
wire_enum!(PullSort { Created = "created", Updated = "updated", Popularity = "popularity", LongRunning = "long-running" });
→
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, concord::WireDisplay)]
#[serde(rename_all = "kebab-case")]
pub enum PullSort { Created, Updated, Popularity, LongRunning }
```

Cloudflare `src/models.rs:12-46` (`identifier!`) and `:64-107` (`provider_enum!`):

```rust
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, concord::WireDisplay)]
#[serde(transparent)]
pub struct ZoneId(String);

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, concord::WireDisplay)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum ZoneStatus { Initializing, Pending, Active, Moved, #[serde(untagged)] Other(String) }
```

Mechanism:
- The derive expands to `impl Display { fn fmt(..) { concord::__private::wire_text(self, f) } }`.
- `wire_text` drives a ~100-line serde `Serializer` that accepts scalars only: str, char, bool,
  numbers, unit variants, newtype structs and untagged newtype variants.
- Anything else is `fmt::Error`, which Concord already turns into a `Build` error naming the
  placement (AUTHOR_GUIDE §6).
- It lives in `concord-codecs` behind `serde`, which `json` and `form` already enable.

**Call site (caller writes):** unchanged. The same text goes on the wire, and `prepare()` URLs
are identical.

**SDK code that disappears:**
- Hand-written `Display` impls that only spell a value: ≈112 impls, ≈675 lines in 32 SDKs. Of
  these, 89 delegate a newtype to its field (≈445 lines) and 23 match a unit enum (≈230 lines).
  These are candidates: not every one is placed in a request.
- Most of the 10 per-SDK macros: cloudflare `models.rs:12-46,64-107,109-125` (≈95);
  github `params.rs:40-61` and `ids.rs:27-31,132-136`; zendesk `models.rs:11-50`;
  paypal `models.rs:461ff`; github `models.rs:20ff`; discord `model.rs:30ff`;
  shopify `ops/mod.rs:47ff`.
- With `#[serde(untagged)] Other(String)` (serde ≥ 1.0.181; the workspace pins 1.0.219), the
  hand-written `From<String>`/`Deserialize` of open enums go too.

Estimate: ≈400–650 lines net, after adding the derive lines (inferred).

**Hardest counterexample:** a type whose text spelling differs from its serde spelling.
Examples: S3 `HttpDate` (IMF-fixdate in `If-Modified-Since`), GitHub `Labels` (comma-joined in a
query, an array in JSON) and Stripe timestamps. These keep their hand-written `Display`. An
author who derives anyway on a sequence gets a `Build` error at `prepare()`, not at compile time.

**Simple control (Nager):** unchanged. Nager has no request-side enum; `HolidayType` is
response-only.

**Charter check:**
- C1: no DSL word.
- C2: the spelling is stated once, in the model, and read by Concord.
- C4: deterministic, because serde's own rules apply.
- C5: misuse on a non-scalar is detected at `prepare()`, not at compile time. This is weaker.
- C6: one sentence of docs.
- C7: a plain `Display`, usable outside Concord.
- C9: unit tests per serde attribute, plus wire tests.

**New concepts:** one derive.

**Breaking changes:** none (opt-in).

**Why it might be wrong:** `serde_plain::derive_display_from_serialize!` already does exactly
this, so a docs recipe might be the whole design. Shipping general-purpose derives drifts toward
a framework.

**Several families:** K (how values become request text) and G (fewer accessors and
constructors). Combined with I6, it also covers acceptance.

## I4. Secret holes (`concord::WireText` and `concord::Secret<T>`)

**Fact it expresses:** "ntfy sends `Authorization: Bearer <access token>`, and the token is
secret". Scheme, placement and secrecy all go in the declaration (charter table: "Auth scheme and
placement").

**Declaration (author writes):** ntfy `src/lib.rs:104-107`:

```rust
pub client Ntfy(
    /// `Authorization`: absent for open topics; [`Ntfy::with_token`] sets a bearer token.
    #[sensitive] header(AUTHORIZATION) authorization?: HeaderValue,
) { … }
→
pub client Ntfy(#[sensitive] token?: AccessToken) {
    headers { AUTHORIZATION = "Bearer {token}"; }
    …
}
```

In Rust, next to the model, this replaces `AccessToken::header_value`:

```rust
impl concord::WireText for AccessToken {
    const SECRET: bool = true;
    fn write_text(&self, out: &mut dyn fmt::Write) -> fmt::Result { out.write_str(&self.0) }
}
```

Stripe (`context.rs:63-66`, `FRICTION.md:155-164`) instead declares
`#[sensitive] key: concord::Secret<String>`.

Mechanism:
- Template holes and text placements are bounded on a public `concord::WireText` instead of the
  hidden `__private::Text: Display` (`__private.rs:55-63`).
- A blanket `impl<T: Display + ?Sized> WireText for T` keeps every current type working.
- An SDK type without `Display` may implement `WireText` itself. The scratch probe confirms this
  coherence.
- `concord::Secret<T>` has no `Display`; its `Debug` is `Secret(<redacted>)`; it has `expose()`;
  it implements `WireText` with `SECRET = true`; and its `ToHeaderValue` sets the sensitive flag.
- For every value used as text, the macro emits
  `const _: () = assert!(!<T as WireText>::SECRET || <declared sensitive>)`. A secret type
  without `#[sensitive]` is then a compile error at the value.

**Call site (caller writes):**

```rust
Ntfy::new(http).with_token(&AccessToken::new(t)?)  →  Ntfy::new(http).set_token(AccessToken::new(t)?)
```

The generated `set_authorization(HeaderValue)` disappears. It let a caller send any scheme
(ntfy `FRICTION.md:119-120`).

**SDK code that disappears:**
- ntfy: `model.rs:95-102` (`header_value`), `lib.rs:166-171` (`with_token`), and the doc lines
  `lib.rs:15,25,105`. That is about 15 lines removed and 4 added.
- Stripe: `context.rs:63-66`, and the `SecretKey::expose` route used only to build the header.
- The 9 SDKs that format `"Bearer {}"` by hand: openai `lib.rs:333-338`, snowflake
  `client.rs:123-125`, microsoft-graph `client.rs:148-150`, kubernetes `ids.rs:242-244`,
  google-drive `client.rs:82`, github `context.rs:257`, linear `lib.rs:258`, ntfy and stripe.
  That is 3–8 lines each.
- Hand-written `Debug` for secret newtypes whose field becomes `Secret<String>` (family D).

Estimate: ≈60–120 lines in about 10 SDKs. Combined with N3 it removes more.

**Hardest counterexample:** a borrowed `HeaderValue` that is already flagged sensitive
(AUTHOR_GUIDE §4.8) is unchanged and stays a native value. A `Secret<HeaderValue>` is not text
(`HeaderValue` has no `Display`), so it cannot fill a template. The rule "a native value is never
written as text" holds. Signing schemes (S3 SigV4, Kraken `API-Sign`) are behavior and stay in
the policy.

**Simple control (Nager):** unchanged (no credentials).

**Charter check:**
- C1: scheme and secrecy are provider facts.
- C2: no refresh logic enters the declaration.
- C4: holes still mean "the value's text".
- C5: a missing `#[sensitive]` on a secret type is a compile error at the value.
- C7: `Secret::expose` and the native `HeaderValue` route remain.
- C9: canary tests (a `Secret` is printed neither by Concord nor by `{:?}`) and a compile-fail
  test.

**New concepts:** `concord::WireText` (a public trait replacing a hidden one) and
`concord::Secret<T>`.

**Breaking changes:** none for current declarations, thanks to the blanket impl for `Display`.

**Why it might be wrong:** SDKs that drop their validated token type move validation from
construction (`AccessToken::new`, `concord_auth::bearer`) to `prepare()`. `Secret<T>` is also
family D's seed 3 and should be decided once, there.

**Several families:** D (secret types' `Debug`; the 7 canary leaks were in SDK-owned types),
C (public `set_authorization(HeaderValue)` setters that admit another scheme) and N3.

## I5. Initialized bodies

**Fact it expresses:** "Kraken accepts a private read with every option absent": a body has a
starting value, like any other value.

**Declaration (author writes):** Kraken `src/lib.rs:55` (also `:59` and `:64`):

```rust
POST balances(body options: BalanceOptions, #[sensitive] query account_id?: String) -> Envelope<Balances> {
→
POST balances(body options: BalanceOptions = BalanceOptions::default(), #[sensitive] query account_id?: String) -> Envelope<Balances> {
```

Mechanism:
- Delete the parse error "a body cannot have a default" (`syntax.rs:1606-1608`).
- An initialized body is stored like an initialized value (`value_inits`, `emit.rs:779-805`). It
  is not positional, `set_options(v)` replaces it, and there is no `maybe_` (it is not optional).
- The language loses an exception rather than gaining a word: `x: T = e` already means "starts
  as `e`" for every other value.

**Call site (caller writes):**

```rust
kraken.account().balances(BalanceOptions::default()).send()  →  kraken.account().balances().send()
kraken.account().open_orders(opts).send()                     →  kraken.account().open_orders().set_options(opts).send()
```

**SDK code that disappears:**
- Kraken `lib.rs:144,149`. `account_snapshot` (`lib.rs:138-153`, 16 classifier glue:forwarding
  lines) simplifies.
- The "Construct with `Default`" doc at `models.rs:103`.
- Both halves of `FIRST_USE.md` F5.
- Test call sites: `tests/first_use.rs:214,531,581,594,599,628`.

That is about 10 SDK lines; the rest is caller side.

**Hardest counterexample:**
- A caller that passes the body positionally today: adopting the initializer turns `balances(o)`
  into `balances().set_options(o)`, a break the SDK chooses per endpoint.
- Dropbox `list_folder(body options: ListFolderOptions)` (`wire.rs:39`) is *not* a candidate. Its
  options have no `Default` (the path is required), and AUTHOR_GUIDE §2 forbids making a required
  value optional just to shorten calls.

**Simple control (Nager):** unchanged.

**Charter check:**
- C1: "every option is optional" is documented by Kraken.
- C3: holds.
- C4: `= expr` reads the same as for values.
- C6: removes a rule.
- C9: the parse test that expects the error flips, and a wire test sends the default body.

**New concepts:** none (one exception fewer).

**Breaking changes:** none until an SDK adopts it.

**Why it might be wrong:** the witness set is small (Kraken, 3 endpoints), and I1 already makes
`balances(Default::default())` compile, which may be enough.

## I6. Identifiers accept borrowed text (`concord::Identifier`)

**Fact it expresses:** "a zone id is opaque text, distinct from a record id". The newtype already
says this; the marker lets Concord accept the text it wraps.

**Declaration (author writes):** unchanged: `scope zone(zone_id: ZoneId)` (Cloudflare
`lib.rs:101`). The model gains `impl concord::Identifier for ZoneId {}`, which requires
`From<String> + Clone`, or the `identifier!` macro (`models.rs:12-46`) emits it.

Mechanism (the Cloudflare `FRICTION.md` F3 seed; the coherence probe compiles):
- `concord-core` adds `impl<T: Identifier> IntoValue<T> for &str`, `… for &String`,
  `… for String`, and `impl<T: Identifier + Clone> IntoValue<T> for &T`, plus the matching
  `Sealed` impls.
- There is no macro change: signatures stay `impl IntoValue<ZoneId>`.

**Call site (caller writes):**

```rust
cf.zone(ZoneId::from(ZONE))                   →  cf.zone(ZONE)
cf.zone(zone.id.clone())                      →  cf.zone(&zone.id)
.set_account_id(AccountId::from(ACCOUNT))     →  .set_account_id(ACCOUNT)
```

**SDK code that disappears:** none in Cloudflare `src`. At call sites: `tests/workflows.rs` lines
58, 88, 113, 134, 187, 195, 202, 239, 279 and 301-304, and `tests/create_consumer.rs:73-133`
(13 sites). Forwarders that only take `&str` and wrap it infallibly are rare, because most wrap
for construction reasons (family C). Estimate: 0–20 SDK lines.

**Hardest counterexample:** it conflicts with I1 for the same type. Under I1, `ZoneId` is exact,
so `&str` cannot be accepted, and `.into()` is the spelling instead. Choosing by trait rather
than lexically would bring back the generic parameter and E0283 (`"z".into()` and
`Default::default()` stay E0283 for identifier inputs). The design must pick per type, and that
is the open question.

**Simple control (Nager):** unchanged (`country_code: String`).

**Charter check:**
- C1: no DSL word; the fact is a trait impl on the model.
- C4: acceptance follows a visible impl.
- C5: an E0277 lists the `Identifier` impls among the accepted types.
- C9: UI tests for `&str`, `&Self` and a non-identifier.

**New concepts:** the `concord::Identifier` marker trait.

**Breaking changes:** none.

**Why it might be wrong:** it improves call sites only, opens a sealed trait in the opposite
direction from I1, and leaves the inference problem untouched.

---

# Family N: removal and simplification

Usage checks (observed, at `ba10189b`):

| Candidate | Users |
|---|---|
| `concord-extras` (a 6-line `pub use concord_codecs::*`) | 0 SDKs or apps. Only `checks/consumers/extras-core` (97 lines) and `extras-legacy` (12) exist to test it. Workspace `publish = false` (`Cargo.toml:10`). |
| `concord_pagination::first_page_then` | 0 callers outside the crate. Its one candidate consumer, Snowflake, could not use it (`FRICTION.md:43-53`: polling already owns partition zero). |
| `concord::Error::meta()` | Every `.meta()` in SDK `src/` resolves to an SDK error type returning `Option<&ResponseMeta>` (airtable `lib.rs:174`, s3 `error.rs:539`, b2 `error.rs:382`, …). Concord's version returns an owned `Option<ResponseMeta>`. Grep is not type-checked, so a deprecation build must confirm. |
| Capability crates labelled "pilot" (`README.md:215-217`, `CHANGELOG.md:305-326`) | auth 20 SDKs; retry 16 SDKs + 2 examples; ratelimit 4 + 2; pagination 39 + 5 |
| `ToHeaderValue` named by SDK code | 0 (only Concord's own tests) |
| V3 removed-spelling diagnostics | 0 users by construction: they are compile errors |

## N1. Delete what nobody uses; stop calling used crates pilots (smaller than you think)

**Fact it expresses:** none; this is a surface removal backed by the usage table above.

**Declaration (author writes):** unchanged in every SDK. Cargo changes only for consumers of the
deleted crate:

```toml
concord-extras = { path = "../../../crates/concord-extras", features = ["json", "form"] }
→
concord-codecs = { path = "../../../crates/concord-codecs", features = ["json", "form"] }
```

The items:
1. Delete the `concord-extras` crate (`crates/concord-extras`: src 6, README 8, Cargo 26 lines)
   and its two consumer checks.
2. Delete `first_page_then` together with `Plan::Waiting` and the `P` parameter of `Independent`
   that exist only for it.
3. Deprecate `Error::meta()` (`error.rs:268-276`), then delete it if the build shows no use.
   `head_meta()` (any head) and `response()`/`into_response()` (the kept response) remain.
4. Change the README/CHANGELOG wording "pilot" for auth, retry and pagination, which are used by
   20, 16 and 39 SDKs. Ratelimit (4 SDKs) stays a pilot.

The facade feature `extras` (used by `examples/renamed/Cargo.toml:13` and
`amazon-s3-sdk/Cargo.toml:11`) is not the crate and stays. Renaming it to `codecs` would be a
separate break of two manifests.

**Call site (caller writes):** no SDK call changes. Downstream code changes `concord_extras::Json`
→ `concord_codecs::Json` (same item; `MIGRATION.md:287` already recommends it) and
`err.meta()` → `err.head_meta()`.

**SDK code that disappears:** 0 SDK lines. Concord lines that go:
- `crates/concord-extras/**` (≈40 lines) and `checks/consumers/extras-core`, `extras-legacy`
  (≈110 lines).
- The extras entries in `Cargo.toml:2`, `scripts/consumers.sh` (lines 32, 59, 68, 76, 90 and
  103-107), `scripts/check-graph.py` (lines 24 and 39-70), `scripts/package-check.sh:27,88` and
  `README.md:188`.
- `concord-pagination/src/independent.rs:63-136` (`first_page_then` and its doctest), plus the
  `Plan::Waiting` arms; `tests/first_page.rs` (243 lines), `tests/diagnostics.rs:131-139`,
  `tests/resume.rs:303ff`; `README.md:79,116`; `lib.rs:35,53,120`.
- `error.rs:268-276` and the cross-references to it in the `head()`/`response()` docs.

That is roughly 500 Concord lines.

**Hardest counterexample:** a removal that some SDK still uses. None of these three has an SDK
user, but grep evidence has misled before: `Operation::is_endpoint()` looked unused and Stripe
`src/send.rs:469` filters with it (`v4/sdk50/cross50/PROPOSALS.md`). The `Error::meta()` result
is grep-only, so it goes through a deprecation build first and is removed only on zero warnings.

**Simple control (Nager):** unchanged (it uses the facade with `json`).

**Charter check:**
- C6: one crate/feature/facade naming hop fewer to explain.
- C7: unaffected, because `concord_codecs` holds the same items.
- C9: `consumers.sh` loses two consumers, and the dependency-graph rules are edited, not
  weakened.

**New concepts:** one crate, one function and one accessor fewer.

**Breaking changes:** the crate (downstream `concord_extras` paths), `first_page_then` and
`Error::meta()`.

**Why it might be wrong:** each item costs almost nothing to keep. Family F may want
first-page-discovered plans (`total_pages` envelopes) once a real consumer fits the shape.

## N2. Retire the V3 migration diagnostics

**Fact it expresses:** none. The macro stops parsing spellings that no longer exist.

**Declaration (author writes):** unchanged for all 55 SDKs, because these spellings do not
compile today. For a stale declaration:

```rust
POST create(item: NewItem via Json)
// today:  "`via` was removed: write `body item: NewItem` and `request Json;` on the endpoint"
// after:  syn's "expected `,`" at `via`, plus one note: "removed in 4.0, see MIGRATION.md §…"
```

What goes:
- `reject_via` and `ViaSite` (`syntax.rs:393-420`, and the calls at `:515,543,1598-1605,1630-1637`).
- In-place output formats (`syntax.rs:528-555`; `model.rs:2575-2587`).
- In-place body formats and `body("media")` (`syntax.rs:1540-1570,1581-1585,1626-1652`;
  `model.rs:2509-2528`), with the `Output.format`, `BodyParam.format` and `BodyParam.media`
  fields.
- Parenthesized media types (`syntax.rs:1166-1175`).
- The fragment-exact `advice()` (`syntax.rs:136-165`), which exists for these messages.
- Tests: the UI cases `format_removed_spellings`, `format_via_removed`,
  `migration_{literal,raw,token}_advice` and `representation_missing_via` (356 lines of `.rs` +
  `.stderr`), and `tests/applied_advice.rs` (145 lines).

**Call site (caller writes):** unchanged.

**SDK code that disappears:** 0 SDK lines. Concord loses ≈180–220 macro lines (inferred from the
ranges above) and ≈500 test lines.

**Hardest counterexample:** a downstream crate still on 3.x syntax gets a generic parse error
instead of an executable suggestion. It is still at the user's token (C5), but loses the advice.
A middle path keeps the single `via` message and drops the rest.

**Simple control (Nager):** unchanged.

**Charter check:**
- C5: the error is still at the user's token, but its text is generic.
- C6: less hidden grammar.
- C9: the removed tests go with the feature they tested. No guarantee is weakened.

**New concepts:** none; internal machinery shrinks.

**Breaking changes:** diagnostics only.

**Why it might be wrong:** this code is stable, isolated and cheap insurance. Removing it buys
maintainer simplicity, not user value.

## N3. The auth scheme is declared: retire `concord_auth::{bearer, authorization}`

**Fact it expresses:** "Cloudflare authenticates with `Authorization: Bearer <api token>`" (the
charter's fact table: "Auth scheme and placement"; behavior column: "Token refresh").

**Declaration (author writes):** Cloudflare `src/lib.rs:61-67`:

```rust
pub client Cloudflare(
    /// `Bearer {api_token}`; build it with [`Cloudflare::connect`] or
    /// `concord_auth::bearer(token)` (sensitive, validated).
    #[sensitive] header(AUTHORIZATION) authorization: HeaderValue,
    policy: SendPolicy,
) {
→
pub client Cloudflare(
    /// The API token.
    #[sensitive] api_token: Secret<String>,
    policy: SendPolicy,
) {
    headers { AUTHORIZATION = "Bearer {api_token}"; }
```

DigitalOcean `lib.rs:24`, Zendesk `lib.rs:48`, Intercom, Sentry, rest-countries and Mastodon
change the same way.

Mechanism:
- No new syntax. Airtable `lib.rs:204`, Dropbox `wire.rs:30`, GitHub `api.rs:61` and Notion
  `api.rs:18` already declare their scheme this way, with a `String`.
- I4 lets the hole hold a `Secret<String>` or the SDK's own token type.
- `concord-auth` keeps `TokenCache` (refresh is behavior) and `basic` (a base64 encoding a
  template cannot express). `bearer` and `authorization` are deprecated, then removed
  (`credentials.rs:50-75`, ≈25 lines).
- Refreshing SDKs set the token instead of a header: QuickBooks `auth.rs:426`, Spotify
  `auth.rs:391`, Strava `auth.rs:221`, Xero `auth.rs:143`, PayPal `auth.rs:195` and HubSpot
  `lib.rs:504` change `set_authorization(bearer(&t)?)` → `set_access_token(Secret::new(t))`.

**Call site (caller writes):**

```rust
Cloudflare::connect(http, token)?
→
Cloudflare::new(http, CloudflareConfig { api_token: Secret::new(token), policy: SendPolicy::default() })
```

`connect` may stay as a convenience without the credential step.

**SDK code that disappears:**
- The credential step and its error plumbing in each static-token constructor: cloudflare
  `lib.rs:143,153` (plus the `Error::Credential` mapping), digitalocean `lib.rs:57,64`, intercom
  `lib.rs:105,112`, sentry `lib.rs:83,91`, zendesk `lib.rs:109,122` (plus `CredentialError`
  variants), rest-countries `lib.rs:38-45` and mastodon `model.rs:104-120`.
- Seven doc lines that restate the scheme ("Sends `authorization: Bearer <token>`, built by
  `concord_auth::bearer`"). This is glue:docs: a declared fact restated in prose.

Estimate: 60–100 lines over 9 SDKs.

**Hardest counterexample:** a removal some SDK still uses:
- Elasticsearch calls `concord_auth::authorization("ApiKey", encoded)` (`model.rs:104-106`). It
  becomes `AUTHORIZATION = "ApiKey {api_key}"`, but the base64 of `id:key` is still computed in
  Rust. That is fine as behavior, but it is not a deletion.
- `basic` (11 SDK files) cannot be a template, so it stays.
- `bearer` refuses spaces and non-ASCII, while `http`'s header rule accepts both. A token with a
  space would reach the wire unless the SDK's token type checks it.

**Simple control (Nager):** unchanged (no auth). PokeAPI and Frankfurter are unchanged too.

**Charter check:**
- C1, C2: the scheme moves into the declaration; refresh stays in Rust.
- C3: holds.
- C5: a bad token is a `Build` error naming the header at `prepare()`, not at construction.
- C9: canaries already cover template-built sensitive headers; one is added for `Secret` holes.

**New concepts:** none beyond I4's `Secret`.

**Breaking changes:**
- Two functions are removed, called from 20 SDK `src` files (listed by
  `grep concord_auth::(bearer|authorization)`).
- `Config` fields change from `authorization: HeaderValue` to the token type.

**Why it might be wrong:** for rotating credentials the SDK still owns the token lifecycle, so
the saving is 1–3 lines per site. And `bearer`'s construction-time validation is a real benefit
that a template loses.

## N4. Header values are text like every other placement (retire the closed `ToHeaderValue`)

**Fact it expresses:** "ntfy reads `X-Priority` as the priority's number". The header placement
is written where the value is declared, like `header("x-title") title?: String` two lines above
it.

**Declaration (author writes):** ntfy `src/lib.rs:117-129` (FRICTION #9: "`ToHeaderValue` is
closed, so `Priority`, `Tags` and `Cache` are sent through templates"):

```rust
POST publish(
    body message: MessageText,
    header("x-title") title?: String,
    priority?: Priority,
    tags?: Tags,
    cache?: Cache,
) -> Event {
    headers {
        "x-priority" = "{priority}";
        "x-tags" = "{tags}";
        "x-cache" = "{cache}";
    }
    request PlainText;
    …
→
POST publish(
    body message: MessageText,
    header("x-title") title?: String,
    header("x-priority") priority?: Priority,
    header("x-tags") tags?: Tags,
    header("x-cache") cache?: Cache,
) -> Event {
    request PlainText;
    …
```

In S3 `api.rs:81-106` (`head_object`), five bare values plus a 7-line `headers { }` block become
five `header("…")` placements.

Mechanism:
- A value placed directly as a header is either a native `HeaderValue` (bytes and flag, as
  today) or any `Display` text (as templates already allow).
- Dispatch happens at the expansion site by autoref: a `HeaderValue` probe (also for `&`, `Box`,
  `Rc`, `Arc` and `Cow` of it) is tried before a `Display` probe. A blanket
  `impl<T: Display> ToHeaderValue for T` beside `HeaderValue` would be E0119, because `http` may
  add `Display`.
- Every member of today's closed set (`header.rs`: str, String, integers, bool, and pointers to
  them) has a `Display` with the same bytes, so nothing changes on the wire.
- Optional extension under the same rule: the `Text` request format encodes any `Display`
  (`concord-codecs/src/text.rs:20` is `Encode<String>` only). This removes ntfy's 8-line
  `PlainText` encoder (`model.rs:177-187`, FRICTION #9).

**Call site (caller writes):** unchanged (`set_priority(Priority::High)`).

**SDK code that disappears:**
- The 40 single-hole header template lines in 6 SDKs: amazon-s3 `api.rs:49-52,99-105,145-151,183-189`
  (≈25 with block lines); stripe `lib.rs:231,245,290,304,393,438`; paypal
  `lib.rs:89,115,130,149`; github `api.rs:63,77,118,174`; ntfy `lib.rs:124-128`.
- The AUTHOR_GUIDE §4.8 sentence "send another type's `Display` text with a template", the
  closed-set docs at `header.rs:13-24`, and the UI test `to_header_value_sealed`.
- DSL lines per endpoint fall (a charter measure).

**Hardest counterexample:**
- **Borrowed `HeaderValue` and aliases:** renamed `lib.rs:30-31` (`Native`, and
  `each tags?: Vec<concord4::reqwest::header::HeaderValue>`). Dispatch is by type, not by
  spelling, so aliases keep the native path. Pointer forms need their own probes.
- **Header order:** AUTHOR_GUIDE §4.7 leaves the order between different header names
  unspecified (the native `HeaderMap` groups by name). Moving a template into the parameter list
  therefore changes nothing a test may rely on.
- **Diagnostics:** for a type with neither form, plain autoref gives "no method found". A
  checked wrapper with `#[diagnostic::on_unimplemented]` is needed to meet C5.

**Simple control (Nager):** unchanged (no headers).

**Charter check:**
- C1: each line is a placement fact; the indirection goes.
- C3: holds.
- C4: one rule ("a header value is native or text").
- C5: the autoref diagnostics are a risk, as above.
- C6: removes a rule and its paragraph.
- C8: same bytes.
- C9: wire tests per former closed-set type, a test that sensitivity is unchanged, and a UI test
  for a type that is neither.

**New concepts:** one public trait fewer (`ToHeaderValue` becomes hidden) and one rule fewer.

**Breaking changes:** code that names `concord::ToHeaderValue` (0 sites outside Concord's own
tests). Values sent today are unchanged on the wire.

**Why it might be wrong:** the closed set may be deliberate: a float, or a type whose `Display` is
meant for humans, never lands in a header by accident. Autoref specialization is clever code in a
library that prizes plain Rust.

---

## How the designs combine

- **Compatible:** I1, I2, I3, I4, I5, N1, N2, N3 and N4 can be taken together. I3 and N4
  reinforce each other: `Display`, or I4's `WireText`, becomes the single contract for a value
  written as text, in paths, queries, templates, headers and `Text` bodies.
- **Conflicting:** I6 conflicts with I1 for the same newtype: borrowed acceptance and exact
  inference cannot both hold for one input in Rust today.
- **Dependent:** N3 works with `String` today but needs I4 for secret token types.
- **Glue removed across several families:**
  - I2 (I, F, G, A): bounded newtypes, validating forwarders, range error variants.
  - I4 + N3 (I, D, C): credential builders, secret `Debug`, setters that admit another scheme.
  - N4 (N, L, K): declaration lines and a concept.
  - I3 (I, K): spelling impls.
- **Glue removed in no family:** I1 and I6 change call sites only. N1 and N2 shrink Concord's own
  surface.
