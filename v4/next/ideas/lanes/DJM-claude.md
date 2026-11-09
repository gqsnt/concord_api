# Idea lane D · J · M (provider: claude)

Families: **D** secrets inside SDK types, **J** observability (Concord logs safely), **M** testing
story for SDK authors. Tree read: `work/wt/rv-p1` at `ba10189b` (read-only). SDK paths below are
relative to `examples/`; core and macro paths to the repository root. The friction catalogue
`work/inventory/FRICTION.md` did not exist; the witnesses' own `FRICTION.md`/`MAINTENANCE.md`
were read instead.

Labels: **measured** = counted on `ba10189b` by a script or `grep` in this lane (method given);
**estimated** = inferred from measured counts, not executed. No design here was implemented or
compiled, except one 10-line coherence probe for D2 (noted there).

## Baseline (measured)

| Fact | Number | How |
|---|---|---|
| Hand-written `fmt::Debug` impls in SDK `src/` | 361 impls, 2,954 lines | brace-matched `impl … fmt::Debug for` (matches the charter) |
| … by shape | 80 opaque `X(<redacted>)` (401 lines); 127 struct with some fields withheld (1,251); 92 struct/tuple printing every field (960); 39 newtype printing its value (198); 23 other | body patterns |
| … by glue label | glue:redaction 190 impls / 1,474 lines; glue:error 44 / 538; glue:result 13 / 110; other 114 / 832 | `classify.py` label of the `impl` line |
| … withholding impls by role in the declaration | 163 on types the declaration never names (errors, metadata, wrappers, cursors); 28 outputs; 14 bodies; ~29 values | name search in the `api!` blocks |
| Declared `#[sensitive]` value types that *also* hand-write a withholding `Debug` (fact written twice) | S3 `ContinuationToken`, Salesforce `Soql`/`QueryLocator`/`ResultLocator`, Mailgun `MessageId`, Kubernetes `ContinueToken`, Stripe `IdempotencyKey`, arXiv `Query`/`IdList`/`SearchTerm`, QuickBooks `CustomerQuery`, USGS `EventId` | cross-reference |
| `#[sensitive]` values | 152, of which 50 are native `HeaderValue` | grep |
| SDKs building a sensitive `HeaderValue` by hand (`set_sensitive`, `concord_auth::*`) | 33 | grep |
| `*_unredacted` / `expose` / `raw_*` accessors | 42 | grep |
| SDKs that log | 0; two apps hand-build request logs (`github-app/src/requests.rs`, `stripe-app/src/reconciliation.rs:168-181,405-413`) | grep |
| Non-doc `request_id` lines | 131 in 20 SDKs | grep |
| Test code (not counted by the glue classifier, which reads `src/` only) | `tests/canary.rs`: 13,004 lines in 55 SDKs; 54 loop over the same 7 scenarios; helper copies: `http()` 66 defs / 567 lines, `refused()` 53 / 170, `concord_error()` 59 / 443, `receive()` 42 / 573; `with_base_url` 352 uses; `TestServer::start` 1,822 uses; no testing section in `docs/` | brace-matched function bodies, grep |

Three things the numbers say before any design:

1. **Most withholding code is not reachable from the declaration** (163 of ~226). Seed D2
   ("sensitivity flows from the declaration") alone cannot remove it; a mechanism on SDK types can.
2. **The SDKs disagree with Concord and with each other.** Placeholders: `<redacted>`,
   `<withheld>`, `<N bytes withheld>`, `(..)`, `..`, `finish_non_exhaustive()`. Europe PMC's
   blanket text-type `Debug` (`europe-pmc-sdk/src/input.rs:41-45`) redacts public
   `PublicationId`s that Concord's own `Prepared` prints (`europe-pmc-sdk/tests/canary.rs:1-7`
   documents the contradiction). ClickHouse's `Param` withholds query values
   (`clickhouse-sdk/src/query.rs:255-260`) that `Prepared` shows (`clickhouse-sdk/src/wire.rs:83-91`).
3. **Secrets need a leaking `Display` to be placed.** ntfy FRICTION §8, Stripe FRICTION F5,
   Kubernetes FRICTION §9: a template hole requires `Display`, so `format!("{token}")` prints the
   secret; 50 values dodge this by being native `HeaderValue`s.

---

# D. Secrets inside SDK types

## D1. `#[derive(concord::Debug)]` with the existing `#[sensitive]` (smaller than you think)

Fact it expresses: which fields of an SDK-owned type are sensitive — the same fact `#[sensitive]`
already states for declared values — printed in Concord's own withholding style.

Declaration (author writes): Twilio, `twilio-sdk/src/models.rs:133-165`. Before:

```rust
#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct Message {
    pub sid: String,
    pub account_sid: String,
    pub status: MessageStatus,
    pub body: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub error_code: Option<i64>,
    pub error_message: Option<String>,
}

/// Shows `sid`, `status` and `error_code` … Withholds the account SID, `from`, `to`, `body` and
/// `error_message` (which can quote submitted data).
impl fmt::Debug for Message {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Message")
            .field("sid", &self.sid)
            .field("status", &self.status)
            .field("error_code", &self.error_code)
            .finish_non_exhaustive()
    }
}
```

After:

```rust
#[derive(Clone, PartialEq, Eq, Deserialize, concord::Debug)]
pub struct Message {
    pub sid: String,
    #[sensitive] pub account_sid: String,
    pub status: MessageStatus,
    #[sensitive] pub body: Option<String>,
    #[sensitive] pub from: Option<String>,
    #[sensitive] pub to: Option<String>,
    pub error_code: Option<i64>,
    #[sensitive] pub error_message: Option<String>,
}
// Message { sid: "SM1", account_sid: <redacted>, status: Queued, body: Some(<redacted>),
//           from: None, to: Some(<redacted>), error_code: None, error_message: None }
```

and a whole value (`twilio-sdk/src/models.rs:11-13,64-68`, the form body): `#[derive(Clone,
Serialize, concord::Debug)] #[sensitive] pub struct CreateMessage { … }` → `CreateMessage { .. }`
(what the hand impl prints today). The whole rule set:

- no attribute: output identical to `#[derive(Debug)]` (so a glob `use concord::*` that shadows
  std's derive changes nothing);
- `#[sensitive]` on a field: `<redacted>`; on a field written `Option<..>`: `None` or
  `Some(<redacted>)` (the presence is shown, as Concord's generated `Debug` already does);
- `#[sensitive]` on the type or an enum variant: `Name { .. }` / `Variant(..)`;
- a withheld field's own `Debug` is never called, so it needs none (native `HeaderValue`, `Url`,
  closures, readers).

Call site (caller writes): unchanged (`println!("{message:?}")`); the text changes as shown.

SDK code that disappears (witness ranges are the classifier's glue:redaction runs plus macro-made
impls):
- twilio: `auth.rs:36-40`, `error.rs:52-59`, `models.rs:64-68`, `models.rs:153-165` (31 lines →
  8 attribute lines; `error.rs:28-37` `Response<T>` becomes a derive);
- salesforce: `bulk.rs:223-230,284-295,398-410`, `bulk_query.rs:590-596`, `context.rs:392-399`,
  `csv.rs:104-125,561-567`, `error.rs:633-644`, `ids.rs:268-272`, `ingest.rs:332-341`,
  `model.rs:291-299`, `soql.rs:347-354`, plus macro-made `model.rs:137-151`, `ids.rs:560-564` (≈140);
- amazon-s3: `error.rs:118-126,307-316`, `sigv4.rs:119-130` (31);
- elasticsearch: `bulk.rs:605-612`, `document.rs:163-171`, `error.rs:375-382,422-437` (incl. the
  SDK's own `struct Withheld`), `model.rs:116-120`, `pit.rs:284-292,423-431` (61);
- shopify: `graphql.rs:387-395`, `ops/catalog.rs:119-134`, `scalars.rs:184-188,208-212,247-253` (42);
- xero-accounting: 13 five-line impls, `auth.rs:58-62`, `error.rs:66-70`, `lib.rs:69-73,148-152,322-326`,
  `models.rs:38-42,60-64,157-161,212-216,249-253,267-271`, `store.rs:79-83,131-135` (65);
- distribution-registry: `auth.rs:165-169`, `client.rs:323-330,377-383`, `model.rs:36-47`,
  `pages.rs:54-58`, `upload.rs:393-404` (49);
- clickhouse: `client.rs:77-89`, `error.rs:186-196,292-303`, `guard.rs:57-65`, `query.rs:255-260` (51);
- github: `context.rs:287-296`, `download.rs:80-89`, `meta.rs:164-176,186-193`, `paging.rs:273-280,581-587` (56);
- mailgun: `error.rs:74-78`, `message.rs:71-75,85-89,176-180,268-272`, `policy.rs:73-79` (32).

Witnesses ≈ 550 lines → ≈ 130 attribute lines. All 55 SDKs (estimated): the 80 opaque and 127
partial impls (1,652 lines) become ≈ 400 attribute lines, **net ≈ −1,250 of 2,954**; many of the
92 impls that print every field by hand (e.g. `distribution-registry-sdk/src/error.rs:309-319`,
`gitlab-sdk/src/meta.rs:140-150`) are std `derive(Debug)` written out and go too if the author
accepts the derived text (not counted). **Other families:** the same derive removes Debug code the
classifier files under A (538 lines in `error.rs` files), B (`Response<T>`/`WithMeta<T>`:
`twilio-sdk/src/error.rs:28-37`, `mailgun-sdk/src/policy.rs:73-79`, `github-sdk/src/meta.rs:186-193`),
C (wrapper clients: `xero-accounting-sdk/src/lib.rs:69-73`, `github-sdk/src/context.rs:287-296`,
`distribution-registry-sdk/src/client.rs:323-330`) and F (cursors: `xero-accounting-sdk/src/lib.rs:322-326`,
`salesforce-sdk/src/bulk_query.rs:590-596`, `github-sdk/src/paging.rs:273-280`).

Hardest counterexample:
- twilio `Message` must show `sid` and `status`: they are simply not marked.
- europe-pmc / clickhouse (identifiers public by design): nothing is inferred from names or
  types. Europe PMC's `text_value!` blanket `Debug` becomes plain derive for `Source` and
  `PublicationId` and `#[sensitive]` only on `Query`/`Cursor`, so the SDK finally agrees with
  Concord's `Prepared`. ClickHouse keeps its choice to withhold the user
  (`clickhouse-sdk/src/client.rs:77-89` → `#[sensitive]` on `Auth::Headers` fields).
- native reqwest values: ClickHouse `Auth::Basic(HeaderValue)` is a `#[sensitive]` variant; the
  value's `Debug` is never called.
- not handled: Shopify `ProductsQuery.query: Var<String>` prints `Absent`/`Null`/`Value(<redacted>)`
  (`shopify-sdk/src/ops/catalog.rs:119-134`); a `#[sensitive]` field prints `<redacted>` and loses
  the three-way distinction — keep that impl by hand, or accept the loss.

Simple control (Nager): unchanged (0 Debug impls, models use std `derive(Debug)`). PokeAPI and
Frankfurter unchanged.

Charter check:
- C1: `#[sensitive]` already means "this value is secret"; no word configures Concord.
- C2: which fields are secret is a fact; nothing decides at run time.
- C3: no declaration changes.
- C4: every withheld field is marked on its own line; no name or type inference except the
  written `Option<..>` spelling.
- C5: `#[sensitive]` on a unit variant or a field of a `union` is an error at the attribute.
- C6: four one-line rules.
- C9: Concord tests a derive corpus with the canary (no marked value in `{:?}`/`{:#?}`) and that
  unmarked output equals std's derive.

New concepts: derive `concord::Debug` (helper attribute `#[sensitive]`, already a Concord word).

Breaking changes: none in APIs; the `Debug` text of adopting types changes (docs that quote exact
Debug strings, e.g. `twilio-sdk/src/lib.rs:65`, must be updated).

Why it might be wrong: A denylist fails open — a field added later prints, which is exactly why
authors hand-write `finish_non_exhaustive()` impls today (D3 answers this). Many withheld fields
are not secrets but content that "can quote submitted data"; reusing `#[sensitive]` for both blurs
a line that matters once Concord logs (J).

## D2. `concord::Sensitive<T>`: sensitivity travels with the value

Fact it expresses: this value is a credential or an opaque token wherever it goes — placed on the
wire, stored in an SDK type, returned by the provider. Written once, on the type, instead of on
every placement.

Mechanism: one core type. `Sensitive<T>`: `Debug` = `<redacted>`; no `Display`; `expose(&self) ->
&T`; `Clone`, serde-transparent (feature `json`). Placement goes through a trait `WireText`
(blanket impl for every `Display` type, so existing code compiles; `Sensitive<T: WireText>` writes
the inner text) instead of the `Display` bound of path segments, query values and template holes
(`crates/concord-core/src/__private.rs:49-57`). `ToHeaderValue` gains `Sensitive<HeaderValue>` and
`Sensitive<String>`, flagged sensitive. A value is withheld when it is marked `#[sensitive]` **or**
`<T as WireText>::SENSITIVE` holds (a const: `false` for the blanket impl, `true` for
`Sensitive<T>`); the macro folds these consts into `EndpointMeta`'s masks at compile time, so type
aliases work and nothing is inferred from a type's name. (Probe in this lane: the blanket impl plus
`impl WireText for Sensitive<T>` is coherent and the const mask evaluates; 10 lines, rustc, not in
the repository.)

Declaration (author writes): ntfy, `ntfy-sdk/src/lib.rs:103-106,165-172` and
`ntfy-sdk/src/model.rs:78-108`. Before:

```rust
pub client Ntfy(
    #[sensitive] header(AUTHORIZATION) authorization?: HeaderValue,
) { base "https://ntfy.sh"; … }

impl Ntfy {
    pub fn with_token(self, token: &AccessToken) -> Self {
        self.set_authorization(token.header_value())
    }
}
pub struct AccessToken(String);
// + header_value(): HeaderValue::try_from(format!("Bearer {}", self.0)) + set_sensitive(true)
// + impl Debug: "AccessToken(<redacted>)"
```

After:

```rust
pub client Ntfy(token?: Sensitive<AccessToken>) {
    base "https://ntfy.sh";
    headers { AUTHORIZATION = "Bearer {token}"; }
    …
}
pub struct AccessToken(String); // new(&str) -> Result<Sensitive<AccessToken>, InvalidToken>; Display = wire text
```

Salesforce (`salesforce-sdk/src/api.rs:39,82-98,118,124`): `pub client SalesforceApi(#[sensitive]
token: String, …)` → `token: Sensitive<AccessToken>`; the eight `#[sensitive]` markers on
`q: Soql`, `locator: QueryLocator` and `locator?: ResultLocator` move onto three type aliases.

Call site (caller writes): `Ntfy::new(http).with_token(&AccessToken::new("tk_x")?)` →
`Ntfy::new(http).set_token(AccessToken::new("tk_x")?)`; reading the secret is
`token.expose()`; `format!("{token}")` no longer compiles.

SDK code that disappears:
- ntfy `model.rs:95-108` (`header_value`, Debug), `lib.rs:165-172` (`with_token`) ≈ 20;
- twilio `auth.rs:31-40` (`into_header`, Debug), `lib.rs:82` (`concord_auth::basic` returns
  `Sensitive<HeaderValue>`) ≈ 9;
- elasticsearch `model.rs:111-120` (`into_header`, Debug of `ApiKey`) ≈ 9;
- salesforce `ids.rs:263-272` (`expose`, Debug), `ids.rs:560-564` (locator Debug), `context.rs:98` ≈ 16;
- amazon-s3 `types.rs:229-239` (`ContinuationToken`'s leaking `Display` and its Debug) ≈ 10;
- kubernetes `ids.rs:207-217`, shopify `scalars.rs:184-188,208-212`, mailgun `message.rs:66-75` ≈ 30;
- distribution-registry `model.rs:27-47` (`TokenBody`: `token: Option<Sensitive<String>>`, std
  derive) ≈ 12.

All SDKs (estimated): most of the 80 opaque impls (≈ 400 lines) plus ≈ 5 lines of header-value
plumbing in each of the 33 SDKs that build a sensitive `HeaderValue` (≈ 150) → **≈ 450–550 lines**,
and the Display leak closes. **Other families:** I (secret values no longer need `Display`), C
(credential newtypes and their `into_header` constructors: `twilio-sdk/src/lib.rs:77-87`), F
(cursors as `Sensitive<_>`: S3 `ContinuationToken`, Kubernetes `ContinueToken`, Shopify `Cursor`).

Hardest counterexample:
- types holding native reqwest values: `Sensitive<HeaderValue>` is placed whole and flagged
  (ClickHouse `clickhouse-sdk/src/wire.rs:25` becomes `authorization?: Sensitive<HeaderValue>`).
- twilio `Message`: not addressed on purpose. Making `account_sid: Sensitive<String>` changes a
  public field type (`.expose()` at every read); models stay on D1/D3, D2 is for credentials and
  opaque tokens.
- europe-pmc: `Query` and `Cursor` become `Sensitive<_>` aliases; `Source`/`PublicationId` stay
  plain. ClickHouse username: the author writes `Sensitive<String>` or not.

Simple control (Nager): unchanged (no secret). PokeAPI, Frankfurter unchanged.

Charter check:
- C1: the type states the fact; the declaration loses markers instead of gaining words.
- C2: no behavior; `expose()` is the caller's decision.
- C3: unchanged.
- C4: visible in the declaration's types; resolved by a trait const, not by spelling.
- C5: `format!("{}", token)` fails with an `on_unimplemented` note naming `expose()`.
- C7: `expose()`, and native `HeaderValue` still works.
- C9: `tests-gen/oracle_sensitivity.py` gains a `Sensitive<T>` axis (withheld in every Concord
  rendering, present on the wire).

New concepts: `concord::Sensitive<T>`, trait `WireText` (replaces the `Display` bound; its
diagnostic keeps today's wording).

Breaking changes: `concord_auth::{basic, bearer, authorization}` return `Sensitive<HeaderValue>`
(33 SDKs, mechanical); the placement bound is named `WireText` in error messages; private
`EndpointMeta` changes from index lists to const masks.

Why it might be wrong: The macro's sensitivity fixpoint (sticky keys) becomes symbolic over type
consts — real complexity in `crates/concord-macros/src/model.rs` for a fact `#[sensitive]` already
states; a cheaper fallback is a const assertion "a `Sensitive` value must be marked" (fact twice,
compiler-checked). Calling cursors and SOQL "sensitive" may be too strong a word for values that
are merely opaque.

## D3. Withheld unless public: `#[sensitive]` on the type, `#[public]` on what may print

Fact it expresses: the provider documents a few fields as public by design (Twilio: "the message
SID is a resource identifier … what Twilio support asks for", `twilio-sdk/src/models.rs:153-156`);
the rest of the record is the customer's content.

Same derive as D1, opposite default for marked types: `#[sensitive]` on a struct withholds every
field except those marked `#[public]`, in today's dominant hand style (`finish_non_exhaustive()`):
withheld field names are not printed. D1's field-level `#[sensitive]` stays available.

Declaration (author writes): Twilio, same before as D1. After:

```rust
#[derive(Clone, PartialEq, Eq, Deserialize, concord::Debug)]
#[sensitive]
pub struct Message {
    #[public] pub sid: String,
    pub account_sid: String,
    #[public] pub status: MessageStatus,
    pub body: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    #[public] pub error_code: Option<i64>,
    pub error_message: Option<String>,
}
// Message { sid: "SM1", status: Queued, error_code: None, .. }  — byte-identical to today
```

Salesforce `QueryJobInfo` (`salesforce-sdk/src/bulk.rs:232-295`, 13 fields, 6 shown): `#[sensitive]`
plus six `#[public]`; a field Salesforce adds next year stays withheld.

Call site (caller writes): unchanged; for the 127 partial impls written with
`finish_non_exhaustive()` the text does not even change.

SDK code that disappears: the same ranges as D1. Attribute cost differs by type: Twilio 4 (D1: 5),
Salesforce `QueryJobInfo` 7 (D1: 9), S3 `Credentials` 2 (D1: 2), GitHub `ResponseMeta` 7 (D1: 1 —
use D1's form there). Net ≈ −1,200 (estimated), same cross-family effects as D1.

Hardest counterexample:
- europe-pmc / clickhouse: "public by design" is exactly what `#[public]` states; the
  counterexample becomes the vocabulary.
- twilio `Message`: shown above.
- native values: never formatted unless `#[public]`.
- GitHub `ResponseMeta` (`github-sdk/src/meta.rs:164-176`) shows 6 of 7 fields; the
  default-withheld form is the wrong tool there, so D3 only works as D1 + one word, never alone.

Simple control (Nager): unchanged.

Charter check:
- C1: `#[public]` is the provider fact "public by design", the counterpart of `#[sensitive]`.
- C4: the default is visible on the type line.
- C6: one more word than D1.
- C9: a corpus test that an unmarked field added to a `#[sensitive]` type is withheld (the
  safe-failure property D1 lacks).

New concepts: helper attribute `#[public]` (on top of D1).

Breaking changes: as D1.

Why it might be wrong: Two defaults mean a reader must check the type line before reading any
field — the kind of rule C6 warns about. `#[public]` next to Rust's `pub` invites "pub field =
printed" misreadings.

## D4. `concord::Link`: a URL the provider hands back is withheld as a type

Fact it expresses: a URL returned by the provider — `Link: rel="next"`, `nextRecordsUrl`, a
signed download `Location`, Shopify's bulk `url` — is a bearer capability or opaque cursor: its
origin is public, its path and query are not.

Mechanism: core type `concord::Link` (an absolute `Url`): `Debug` prints the origin only
(`Link(https://objects.githubusercontent.com/..)`); no `Display`; `origin()`, `expose() -> &Url`;
serde `Deserialize` (feature `json`) and `Link::from_header`. `Prepared::follow` accepts a `&Link`
(same-origin rule unchanged, `crates/concord-core/src/prepared.rs:62-77`, which already withholds
a followed URL whole). J's renderings print its origin only.

Declaration (author writes): Shopify, `shopify-sdk/src/scalars.rs:214-260`. Before:

```rust
pub struct SignedUrl(Url);
impl SignedUrl {
    pub fn parse(url: &str) -> Result<Self, InvalidValue> { … }
    pub fn host(&self) -> &str { … }
    pub fn expose(&self) -> &Url { … }
}
impl fmt::Debug for SignedUrl { /* host only, finish_non_exhaustive */ }
impl<'de> Deserialize<'de> for SignedUrl { /* parse */ }
```

After: `pub type SignedUrl = concord::Link;` (model fields unchanged). GitHub `NextLink`
(`github-sdk/src/paging.rs:273-280`): `url: concord::Link`, std `derive(Debug)`.

Call site (caller writes): `prepared.follow(next.url().as_str())?` → `prepared.follow(next.url())?`;
reading the URL stays explicit (`expose()`).

SDK code that disappears: shopify `scalars.rs:214-260` (≈ 40); github `paging.rs:273-280`,
`download.rs:69-72,80-89`, `meta.rs:72,105,148-152,173`; distribution-registry `pages.rs:39-58`
and the origin/userinfo checks of `client.rs:286-312`; salesforce `model.rs:31-41,185,190-199`.
≈ 120–150 lines in about 7 SDKs (estimated; Microsoft Graph `links.rs` and DigitalOcean
`pagination.rs` not read in detail). **Other families:** F (the same type is the next-page
cursor), J (logs print the origin), C (GitHub's download-host checks).

Hardest counterexample:
- GitHub `ArtifactLocation` must show the download host (`github-sdk/src/download.rs:80-89`):
  the origin is shown.
- Registry must refuse a cross-origin `Location`: `follow` already enforces same origin; a
  cross-origin signed download is executed natively.
- twilio / europe-pmc: unaffected. Native values: `Link` wraps reqwest's `Url` and exposes it.

Simple control (Nager): unchanged. PokeAPI keeps `next: Option<String>` (a public URL, nothing
forces the type).

Charter check:
- C1: "server-issued URL" is the fact.
- C2: following and paging stay in Rust (`concord-pagination`).
- C3: unchanged.
- C7: `expose()`.
- C9: canary in the path and query of a link; absent from `Debug` and logs, present on the wire
  after `follow`.

New concepts: `concord::Link`.

Breaking changes: none (`follow` takes a sealed `FollowTarget` implemented for `&str` and `&Link`).

Why it might be wrong: Only ≈ 7 SDKs withhold such URLs today; for public APIs the type adds
ceremony. Family F may declare the cursor location and make SDK link types unnecessary, leaving
`Link` only for signed downloads.

## D, rejected seed: sensitivity flowing from the declaration into SDK types (seed 2)

The macro cannot give `Soql` a redacted `Debug` because `#[sensitive] q: Soql` appears in the
declaration: it would emit an impl for a type it does not own (conflicting with the author's
derive, duplicated across two declarations, impossible for `String`/`HeaderValue`), and 163 of the
~226 withholding impls are on types no declaration names. D2 takes the reverse direction (the type
states the fact, the declaration inherits it), which is implementable.

---

# J. Observability: Concord logs safely

Observed starting point: no SDK logs; Concord's redaction lives in `Prepared`'s `Debug`
(`crates/concord-core/src/prepared.rs:323-366`) and stops at `into_parts()` ("one-way door",
`prepared.rs:19-22`); every execution, including a policy's after `into_parts`, goes through
`Decoder::execute` (`prepared.rs:600-611`), whose `Decoder` still carries the `Operation` and its
`&'static EndpointMeta`. `EndpointMeta` has no route template
(`crates/concord-core/src/__private.rs:19-44`). Two apps restate operation names as string
literals to keep a request log.

## J1. Static-only call events (smaller than you think)

Fact it expresses: nothing new — the declared identity (`Client.scope.endpoint`, method) and route
template the declaration already states, plus observed status, timing and delivery.

Mechanism: feature `tracing` of concord-core, off by default. `Decoder::execute` emits one event
(target `concord`) when the head arrives or the transport fails; `Decoder::decode_keeping`
(`prepared.rs:658-718`) one when the body is consumed or fails. Fields are `&'static str` or
integers only: `operation`, `method`, `route` (new `EndpointMeta::route`, e.g.
`repos/{owner}/{name}/issues`, emitted by the macro from the composed templates it already holds,
`crates/concord-macros/src/emit.rs:3061-3093`), `status`, `elapsed_ms`, `delivery`, `error_kind`.
No URL, header, body or value can be written into an event: the field types make the leak
unwritable, and the canary test confirms it.

Declaration (author writes): unchanged (`github-sdk/src/api.rs:50-68`). The application writes, in
`Cargo.toml`, `concord = { version = "4", features = ["tracing"] }`.

Call site (caller writes): before (`stripe-app/src/reconciliation.rs:405-413`):

```rust
fn record(report: &mut ReconciliationReport, operation: &'static str, meta: &ResponseMeta) {
    report.requests.push(RequestRecord { operation, status: meta.status, request_id: …, … });
}
// call sites pass operation: "GET /v1/invoices" by hand
```

After: `tracing_subscriber::fmt().with_env_filter("concord=info").init();` →
`INFO concord: operation="Stripe.invoices.list" method="GET" route="v1/invoices" status=200
elapsed_ms=84 delivery="Answered"`.

SDK code that disappears: none in SDKs (none logs today, measured). Apps: `github-app/src/requests.rs:11-33,45-65`
(operation and status part), `stripe-app/src/reconciliation.rs:168-181,405-413` ≈ 60 lines (the
request-id part needs J4). Docs: the "Not covered: … tracing" item of CALLER_GUIDE §8. **Other
families:** the same `route` string is the "method and URL template" line of family H's doc budget
and M3's composition.

Hardest counterexample:
- zero cost when off: the code is behind a cfg in concord-core; the macro's output is unchanged
  except one `&'static str` in a static that already exists. On and unsubscribed: tracing's
  per-callsite interest cache, one atomic load.
- path holes and host labels: never logged; the route keeps `{name}`.
- requests edited natively (S3's signer canonicalizes the target after `into_parts`,
  `amazon-s3-sdk/src/client.rs:67-69`; GitHub follows `Link` URLs): no part of the URL is logged,
  so nothing can leak.

Simple control (Nager): unchanged (declaration, constructor, tests).

Charter check:
- C1: no new word.
- C2: whether and where to log is the app's subscriber.
- C3: unchanged.
- C7: nothing removed.
- C8: observation only, off by default, request bytes unchanged.
- C9: the canary harness with a capturing subscriber over the `oracle_sensitivity.py` corpus — no
  canary in any event; positive control: status present.

New concepts: feature `tracing`, event target `concord`, fixed field names.

Breaking changes: none (additive; `EndpointMeta` is private).

Why it might be wrong: Static events may be too poor to diagnose ("which repository?"), so
operators will print `{prepared:?}` themselves, outside Concord's guarantee (J3 answers this). A
global tracing dispatcher may count as "global state" under a strict reading of C8.

## J2. `Summary`: the redacted rendering as a value, kept through `into_parts`

Fact it expresses: nothing new; it exposes the redaction Concord already computes
(`redacted_url`, `crates/concord-core/src/__private.rs:660-726`) as one `Display` line any logger
can take.

Mechanism: `prepared.summary()`, `received.summary()`, `error.summary()` and, after
`into_parts`, `decoder.summary(&request)` return `concord::Summary<'_>`:
`GET https://api.github.com/repos/rust-lang/rust/issues?state=open -> 200`. Two rules beyond
`Prepared`'s `Debug`: (1) the `Decoder` keeps a 64-bit fingerprint of the URL it built; a request
whose URL no longer matches prints `https://host/<withheld>` (as `Prepared::follow` does today,
`prepared.rs:28-31`); (2) values written by `query(..)` pairs are withheld, as the endpoint's own
`Debug` already withholds them. Response side: status, and the declared request id (J4); no other
header value.

Declaration (author writes): unchanged.

Call site (caller writes): before, nothing Concord-safe to log after `into_parts`; after,
`tracing::info!(call = %prepared.summary())` or `log::warn!("{}", error.summary())`.

SDK code that disappears: ≈ 0 in SDKs directly. SDK error `Display`s that restate
"operation: HTTP status" (e.g. `clickhouse-sdk/src/error.rs:168-184`, `stripe-sdk/src/send.rs:405-430`)
can delegate to `error.summary()` — counted under family A, not here.

Hardest counterexample:
- clickhouse: `query(..) params` (`clickhouse-sdk/src/wire.rs:46`) hold application data that
  `Prepared`'s `Debug` prints (`wire.rs:83-91`) and the SDK's `Param` withholds; rule (2) withholds
  them in the summary. Whether `Prepared`'s `Debug` should follow is left open.
- S3 (native edit after `into_parts`): the fingerprint mismatch withholds path and query
  (over-redaction, safe).
- host labels: sensitive labels redacted exactly as in `Prepared`'s `Debug`.
- zero cost: nothing runs unless called.

Simple control (Nager): unchanged.

Charter check:
- C2: renders facts; the app decides to log.
- C7: redaction survives `into_parts`, which it does not today.
- C9: oracle check that the summary URL equals `Prepared`'s redacted URL, plus native-edit and
  `query(..)` cases.

New concepts: `concord::Summary`, `summary()` on four types.

Breaking changes: none (`Decoder` grows 8 bytes).

Why it might be wrong: A logger-agnostic API leaves every app to wire it, so "Concord logs itself"
is not met by J2 alone. Non-sensitive path values (repository names, an e-mail in a path) are
still printed, and operators may not expect them in a central log.

## J3. One span per call; fields come from the declaration's own classification

Fact it expresses: nothing new — `#[sensitive]`. A value Concord's generated `Debug` shows today
becomes a span field; a withheld one is not a field at all.

Mechanism: with feature `tracing`, the generated `send()` (`emit.rs:2059`, and around `async move
{ #call }` at `emit.rs:2163` when a policy exists) enters span `concord.call` with `operation`,
`method`, `route`, and one field per value of the endpoint and its ancestors that `debug_fields`
(`emit.rs:1693-1747`) would show; sensitive values, `query(..)` pairs and natively sensitive
header values are omitted. J1's events become children, so a policy's retries and sub-requests are
attempts under one call. The macro calls a `macro_rules!` exported by concord-core whose body is
empty when the feature is off (decided in concord-core, so SDK crates need no feature).

Declaration (author writes): unchanged. GitHub `github-sdk/src/api.rs:50-58` gives, for
`repo("rust-lang", "rust").issues().set_state(Open).send()`:
`concord.call{operation="GitHub.repo.issues" method="GET" route="repos/{owner}/{name}/issues"
owner="rust-lang" name="rust" state="open"}` — `token` absent.

Call site (caller writes): only the subscriber (as J1).

SDK code that disappears: ≈ 0 in SDKs; apps: most of `github-app/src/requests.rs` (per-call
records, attempts), `stripe-app/src/reconciliation.rs:168-181,405-413` and the print loop
`stripe-app/src/bin/st_a.rs:90-95` ≈ 100 lines.

Hardest counterexample:
- path holes and host labels: a sensitive segment's value is not a field; the route keeps the
  placeholder.
- PII that is not marked: the rule is "logged iff `{endpoint:?}` shows it". Mailgun marks
  `recipient` (`mailgun-sdk/src/lib.rs:87`); Twilio's `to` is in the body and bodies are never fields.
- zero cost when off: the exported macro expands to nothing; values are not evaluated.
- S3's cloned signer: fields are recorded by reference at span creation, no clones.

Simple control (Nager): unchanged; with the feature on, a call logs `country_code` and `year`,
which `{endpoint:?}` already prints by design (`nager-holidays-sdk/tests/canary.rs:5-7`).

Charter check:
- C1: the classification is `#[sensitive]`'s.
- C3: unchanged.
- C4: one sentence ("logged iff shown by `Debug`").
- C8: observation only.
- C9: the oracle corpus captures span fields; withheld values absent, shown values present.

New concepts: span `concord.call`; the rule "logged iff shown".

Breaking changes: none.

Why it might be wrong: It turns "Concord prints this in a local `Debug`" into "Concord ships this
to a log aggregator": same classification, much wider audience. Authors who left PII unmarked
because `Debug` was local will leak at scale. Per-endpoint field lists add code size when the
feature is on (Kubernetes).

## J4. Declared correlation header: `request_id "x-github-request-id";`

Fact it expresses: the provider identifies each response by this header, and it is safe to print —
support asks for it (GitHub `x-github-request-id`, S3 `x-amz-request-id` + `x-amz-id-2`, Stripe
`request-id`, GitLab `x-request-id`).

Mechanism: a client (or scope) statement `request_id NAME, …;` (literal or `HeaderName` constant,
checked like header keys). Core reads it through the `Operation`: `ResponseMeta::request_id()`,
`Received::request_id()`, `Error::request_id()`; `Error`'s `Display` appends `(request id …)`; J1
and J3 carry it as a field. It is the only response header value Concord ever prints.

Declaration (author writes): GitHub, `github-sdk/src/api.rs:50-68`. Before:

```rust
pub client GitHub(#[sensitive] token: String, …) {
    base "https://api.github.com";
    headers { AUTHORIZATION = "Bearer {token}"; … }
```

After:

```rust
pub client GitHub(#[sensitive] token: String, …) {
    base "https://api.github.com";
    request_id "x-github-request-id";
    headers { AUTHORIZATION = "Bearer {token}"; … }
```

S3 (`amazon-s3-sdk/src/api.rs:33-38`): `request_id "x-amz-request-id", "x-amz-id-2";`.

Call site (caller writes): `page.meta.response.request_id()` (SDK type) → `meta.request_id()`
(Concord's); error messages carry it with no SDK code.

SDK code that disappears: github `meta.rs:31-32,66,93,114-117,168`; amazon-s3
`error.rs:134-135,149-150,160-168,180-181`; gitlab `meta.rs:60,84,109-112`; stripe
`send.rs:86,116,418-420,426-428`; apps: `github-app/src/requests.rs:18-19,29`,
`stripe-app/src/reconciliation.rs:175-176,409`. Upper bound 131 non-doc `request_id` lines in 20
SDKs (measured), ≈ 100 removable (estimated). **Other families:** A (error `Display` and
accessors), B (metadata wrappers lose a field; with more declared response facts they can go).

Hardest counterexample:
- zero cost when off: one `&'static [&str]` per client; read only by accessors and `Display`.
- S3's request id inside the XML error document (`amazon-s3-sdk/src/error.rs:212,249-250,298`):
  a body fact, stays SDK code (family A).
- a header that carries a secret cannot be printed by accident: only declared names print.

Simple control (Nager): declares none; unchanged.

Charter check:
- C1: a provider fact.
- C2: what to do with it is the app's.
- C3: unchanged.
- C4: one statement.
- C5: an invalid header name is reported at the statement (and by `validate_header_names()`).
- C9: canary in an undeclared response header never printed; the declared one present.

New concepts: statement `request_id`.

Breaking changes: `Error`'s `Display` text gains a suffix in SDKs that adopt it (tests comparing
exact strings).

Why it might be wrong: It is the first response-side fact; families B/E (rate-limit header names,
`retry-after`) want the same slot, and one "declared response headers" design should own it
instead of a single-purpose statement. Changing `Error`'s `Display` touches every test that
compares it.

---

# M. Testing story for SDK authors

Observed starting point: every SDK depends on two unpublished crates,
`examples/testserver` (1,240 lines) and `checks/canary` (219), and re-writes the same helpers
around them (baseline table). `docs/` has no testing section; the only generated test helper is
`validate_header_names()`. Tests reach the fixture by replacing the base (`with_base_url`, 352
uses), plus `reqwest::ClientBuilder::resolve` where subdomains are declared
(`amazon-s3-sdk/tests/common/mod.rs:67-81`, `github-sdk/tests/support/mod.rs:48-82`).

## M1. `concord-test`: publish what every SDK already copies (smaller than you think)

Fact it expresses: none (test infrastructure; no declaration change).

Mechanism: move `examples/testserver` and `checks/canary` into one published dev-dependency,
`concord-test`, and add the helpers SDKs re-write:
- `strict_client()` — no proxy, no redirects, `retry::never()`, 2 s timeout (66 copies of `http()`);
- `refused_url()` (exists; 53 wrappers);
- `Canaries::concord_error(&concord::Error)` — kind, head, head_meta, operation, source chain (59 copies);
- `Canaries::receive(prepared).await` — receive, head and meta checks, decode (42 copies);
- `wire(&prepared) -> Recorded` — the same `Recorded` view built from `prepared.request()`, with
  no server, for tests that only check composition;
- `Routes` — host + method + target to scripted replies (Salesforce FRICTION F14: a ~70-line router).

Declaration (author writes): unchanged. Tests before (`twilio-sdk/tests/canary.rs:14-26,44-64`):

```rust
fn http() -> concord::reqwest::Client {
    concord::reqwest::Client::builder().no_proxy()
        .redirect(concord::reqwest::redirect::Policy::none())
        .retry(concord::reqwest::retry::never())
        .timeout(Duration::from_secs(2)).build().unwrap()
}
fn refused() -> String { concord_example_testserver::refused_url() }
async fn receive<T>(c: &mut Canaries, prepared: concord::Prepared<T>) -> Result<T, concord::Error> { /* 11 lines */ }
fn concord_error(c: &mut Canaries, error: &concord::Error) { /* 7 lines */ }
```

After:

```rust
use concord_test::{strict_client, refused_url, Canaries};
let native = c.receive(read).await;
```

Call site (caller writes): n/a (SDK tests).

SDK code that disappears (test code; the glue classifier does not count it): the four helpers,
≈ 1,750 lines across 55 SDKs (measured: 567 + 170 + 443 + 573); Twilio `tests/canary.rs:14-26,44-64`
and `tests/support/mod.rs:31-39` ≈ 38; Salesforce's router (FRICTION F14). Concord side: the
Kubernetes FRICTION §10 Nagle delay (~40 ms per response) is fixed once (`set_nodelay`).

Hardest counterexample: SDKs whose client is not the strict one (GitHub resolves hosts and uses
its own redirect policy, `github-sdk/tests/support/mod.rs:48-66`) use `strict_builder()` and add
to it. Paused-clock tests (`examples/testserver/src/paused.rs`) move unchanged.

Simple control (Nager): `tests/canary.rs:18-30` and its `concord_error` helper go (≈ 25 lines);
declaration unchanged.

Charter check:
- C3: nothing in declarations.
- C7: `strict_client()` returns a plain `reqwest::Client`.
- C9: the canary becomes a public, versioned tool for SDK authors (seed D4) instead of an internal
  check.

New concepts: crate `concord-test` and six helpers.

Breaking changes: none for callers; SDK dev-dependencies are renamed.

Why it might be wrong: Publishing the test server makes 1,240 lines (paused clock, gates, body
generators) a stability promise. The savings are in tests, which the charter's glue metric does
not count; the owner may rank them below `src/` glue.

## M2. The seven failure shapes as a conformance matrix

Fact it expresses: none; Concord names what every SDK must survive — 2xx, 4xx with error document,
5xx, malformed body, wrong `Content-Type`, truncated body, refused connection — the I2 scenario set
54 of 55 `canary.rs` already loop over (`for case in 0..7`).

Mechanism: `concord_test::Shape::ALL` with `shape.reply(success, problem, echo) -> Reply` and
`shape.expected()` (the Concord outcome each shape must become: `Ok`, `ErrorKind::{Status, Decode,
Metadata, Transport}` with its `Delivery`). A driver runs each shape against a fresh server, applies
the canary absence checks to every rendering of the outcome through a small `Inspect` trait the
SDK result implements (`meta()`, `concord()`, `provider()`), and returns the recorded requests.

Declaration (author writes): unchanged. Test before (`twilio-sdk/tests/canary.rs:28-42,108-123`):

```rust
fn reply(case: usize, status: u16, success: &str, problem: &str, echo: &str) -> Reply {
    let response = match case {
        0 | 6 => Reply::json(status, success),
        1 => Reply::json(401, problem),
        2 => Reply::json(500, problem),
        3 => Reply::json(status, format!("{{invalid {echo}")),
        4 => Reply::text(status, "text/html", echo),
        5 => Reply::json(status, "").with_body(Body::FailAfter(vec![format!("{{\"incomplete\":\"{echo}").into()])),
        _ => unreachable!(),
    };
    response.header("x-canary", echo).header("set-cookie", echo)
}
for case in 0..7 { /* TestServer::start, with_base_url(if case == 6 { refused() } else { server.url() }) … */ }
```

After:

```rust
concord_test::conformance(&mut c, Bodies { success: DELIVERED_WITH_ECHO, problem: UNAUTHORIZED_WITH_ECHO },
    |base| async move { client(&base).get_message("SM1").await },
    |shape, outcome| assert_twilio_mapping(shape, outcome)).await;
```

Call site (caller writes): n/a.

SDK code that disappears: `reply()` (≈ 15 lines) plus ≈ 10 lines of loop scaffolding in 54 canary
files ≈ 1,300 test lines (estimated from Twilio 14 and Nager 19 lines; brace-matching overcounts
bodies that contain JSON braces); plus the one-shape-per-test protocol tests that re-check the same
failures (e.g. Twilio's malformed, HTML and truncated cases in `tests/protocol.rs`).

Hardest counterexample: providers with other failure shapes — Slack 200 `ok:false`, ClickHouse's
exception after a 2xx head (`clickhouse-sdk/src/guard.rs`), Snowflake 202 — the seven are a floor
and SDK shapes are added as extra replies. Twilio's `Message` shows `sid`: the echo goes only into
withheld fields because the SDK writes the success body with an `{echo}` hole where it chooses.

Simple control (Nager): `tests/canary.rs:32-50` and its loop collapse to one call.

Charter check:
- C6: seven named shapes, no paragraph.
- C9: the guarantee every SDK claims gets one definition.

New concepts: `Shape`, `conformance()`, trait `Inspect`.

Breaking changes: none.

Why it might be wrong: `Inspect` is glue moved into tests unless family A makes SDK errors
Concord-shaped first. A standard matrix may make authors stop thinking about provider-specific
failures.

## M3. `composition()`: the declaration's wire plan as a reviewed snapshot

Fact it expresses: nothing new — a deterministic text of what the declaration states per endpoint:
method, URL template, query keys in wire order, header names with template and sensitivity,
request/response format and media type, limit. Templates only, never values, so the file is safe
to commit.

Mechanism: the macro emits `Client::composition() -> &'static str` (a sibling of
`validate_header_names()`); `concord_test::assert_composition!(TwilioApi, "tests/twilio.composition")`
compares it with a checked-in file; `CONCORD_BLESS=1` rewrites it.

Declaration (author writes): unchanged. Test before (`twilio-sdk/tests/protocol.rs:21-28,58-62,97-111,162-192`,
declared facts restated against the wire):

```rust
assert_eq!(request.method, "POST");
assert_eq!(request.target, CREATE_PATH);   // "/2010-04-01/Accounts/AC_fixture_account/Messages.json"
assert_eq!(request.header_all("authorization"), [AUTHORIZATION]);
assert_eq!(request.header_all("content-type"), ["application/x-www-form-urlencoded"]);
assert_eq!(request.header_all("accept"), ["application/json"]);
```

After:

```rust
#[test]
fn composition() { concord_test::assert_composition!(TwilioApi, "tests/twilio.composition"); }
```

```text
POST TwilioApi.create_message
  url     https://api.twilio.com/2010-04-01/Accounts/{account_sid: sensitive}/Messages.json
  header  authorization = {authorization} (sensitive)
  header  accept = application/json (Json)
  body    message: CreateMessage as application/x-www-form-urlencoded (Form)
  ->      Message (Json)
GET TwilioApi.get_message
  url     https://api.twilio.com/2010-04-01/Accounts/{account_sid: sensitive}/Messages/{sid}.json
  …
```

Protocol tests keep what the SDK itself computes (the form bytes of `CreateMessage`, error mapping).

Call site (caller writes): n/a.

SDK code that disappears: declared-fact assertions — in the witnesses 275 lines (measured: Twilio
37, Elasticsearch 51, Registry 74, Xero 36, Mailgun 19, Nager 11, …); across SDKs 1,042 assertion
lines on method/target/accept/content-type/authorization, of which about half restate declared
facts (others check SDK-computed values such as S3's SigV4 `authorization`) → ≈ 500 test lines
(estimated); Twilio `tests/support/mod.rs:10-13` (`CREATE_PATH`, `FETCH_PATH`). **Other families:**
H (the same text is the "method and URL template" doc line), J (route), K (query order becomes
visible in review — the H16 surprise).

Hardest counterexample:
- clickhouse: `query(..)` pair keys are chosen at run time (`clickhouse-sdk/src/wire.rs:46`), so
  the composition shows `query(..) params` only; their keys stay a protocol assertion.
- europe-pmc: public IDs appear as `{id}` placeholders, never values.
- native header keys (`header(X_TENANT)`): rendered by constant name.

Simple control (Nager): optional one test line and a six-line file; `tests/support/mod.rs:22-33`
(`assert_read`) loses its method/target/accept lines; declaration unchanged.

Charter check:
- C1: no declaration words.
- C4: the file shows exactly what a reader should predict from the declaration.
- C6: the text must read without docs (risk).
- C9: Concord's `tests-gen/oracle_composition.py` proves rendering = built request; the SDK test
  proves the declaration did not change unnoticed.

New concepts: generated `composition()`, test macro `assert_composition!`.

Breaking changes: none (an endpoint named `composition` follows the collision rule of
`validate_header_names`).

Why it might be wrong: A snapshot catches change, not error — a wrong path blessed once is as
green as a right one, while today's hand assertions at least restate the provider docs. The text
format becomes a compatibility surface: a Concord release that changes it re-blesses 55 files.

## M4. Tests reach the declared origin: no base swapping

Fact it expresses: none new; tests exercise the declared base, scheme and subdomain labels exactly
as the provider is addressed.

Mechanism: `concord_test::Origin::start(responder)` runs the test server also as an HTTP CONNECT
proxy with a throw-away CA. `origin.client()` is `strict_builder()` + `Proxy::all(origin.proxy_url())`
+ `tls_certs_only([origin.ca()])` (`tls_certs_only` is already used by
`crates/concord/tests/tls.rs:83-89`). For `https://api.twilio.com` the proxy accepts
`CONNECT api.twilio.com:443`, mints a leaf certificate for that name, terminates TLS and records
the request with `host = "api.twilio.com"`; plain-http bases use absolute-form proxying. No DNS
override (which loses the port: `github-sdk/tests/support/mod.rs:68-71`), no `with_base_url`.

Declaration (author writes): unchanged. Test before (`twilio-sdk/tests/support/mod.rs:41-49`;
GitHub `tests/support/mod.rs:48-82`):

```rust
pub fn client(base: &str) -> Twilio {
    Twilio::new(http(), ACCOUNT, BasicAuth::new("fixture-user", "fixture-password").unwrap())
        .with_base_url(base).unwrap()
}
// GitHub: .resolve(API_HOST, api) + .with_base_url(format!("http://{API_HOST}:{}", api.port()))
```

After:

```rust
let origin = concord_test::Origin::start(move |_| script.next()).await;
let twilio = Twilio::new(origin.client(), ACCOUNT, auth); // the declared https://api.twilio.com
```

Call site (caller writes): n/a.

SDK code that disappears: tests — most of the 352 `with_base_url` uses; S3
`tests/common/mod.rs:67-81` (host resolution loop); GitHub `tests/support/mod.rs:48-82` (`http`,
`http_with`, `api_base`, `context`); Datadog `tests/protocol.rs:41`. `src/`, together with family
C — `with_base_url` wrappers kept for fixtures: Twilio `lib.rs:89-102` ("Use the loopback test
server's URL"), DigitalOcean `for_fixture`, USGS's fixture-only base (CONSTRUCTION_PROBLEM table),
≈ 15 lines where present. **Other families:** C — this lets the base be a fixed provider fact
(FM3 Salesforce `with_base_url(evil)`, FM2 Zendesk) without a test-only constructor; the remaining
base changes are real deployments (self-hosted ntfy, Elasticsearch, ClickHouse, Registry), which
the declaration can state.

Hardest counterexample:
- S3 SigV4 signs the `Host` header: against the declared virtual host the test verifies the real
  canonical request, an improvement over `s3.test:<port>`.
- the refused-connection shape: through a proxy it becomes a failed CONNECT, still
  `Delivery::Unknown` but `transport().is_connect()` may differ; that shape keeps `refused_url()`
  with a direct strict client.
- tests of `with_base_url` itself (`twilio-sdk/tests/first_use.rs:1068-1091`) stay while the SDK
  offers the method.

Simple control (Nager): `tests/support/mod.rs:10-19` becomes `NagerHolidays::new(origin.client())`;
declaration unchanged.

Charter check:
- C7: the caller's `reqwest::Client` stays the only seam (proxy and roots are client settings).
- C8: nothing is hidden in Concord; the redirection is in the test's client.
- C9: tests check the declared URL, not a substituted one.

New concepts: `concord_test::Origin` (proxy + CA).

Breaking changes: none in Concord; dev-dependencies gain rustls/rcgen.

Why it might be wrong: TLS minting and a CONNECT proxy are 300+ lines of test infrastructure with
their own failures, and every test pays a handshake (on top of the measured 40 ms Nagle delay
until M1 fixes it). Tests that deliberately use a path prefix or plain http stay on
`with_base_url`, so the method cannot vanish for SDKs that offer it.

## M5. Canary-filled models: one line checks a type's withholding

Fact it expresses: none; it tests D1/D3's per-field fact.

Mechanism: `concord_test::fill::<T: DeserializeOwned>(&mut Canaries) -> T` deserializes `T` from a
synthetic self-describing source that answers every string with a fresh canary, numbers with 1,
`Option` with `Some`, sequences with one element, enums with their first variant;
`fill_with::<T>(&mut c, [("status", "queued")])` overrides validated fields.
`Canaries::assert_withheld(&value)` checks `{:?}` and `{:#?}` contain no canary except those placed
in fields the `concord::Debug` derive lists as shown (it emits `const SHOWN: &[&str]`).

Declaration (author writes): unchanged. Test before (`twilio-sdk/tests/canary.rs:116-118` plus a
full HTTP round trip to reach `assert_debug_absent("decoded Message", &r.value)` at line 84):

```rust
let success = format!(r#"{{"sid":"SM_one","account_sid":"{response_account}","status":"queued","to":"{to}","from":"{from}","body":"{body}","error_message":"{echo}"}}"#);
let problem = format!(r#"{{"status":401,"code":20003,"message":"{echo}","more_info":"https://example.test/{echo}"}}"#);
```

After:

```rust
#[test]
fn models_withhold() {
    let mut c = Canaries::new("twilio");
    c.assert_withheld(&concord_test::fill_with::<Message>(&mut c, [("status", "queued")]));
    c.assert_withheld(&concord_test::fill::<ProviderError>(&mut c));
}
```

Call site (caller writes): n/a.

SDK code that disappears: the hand-built echo fixtures and per-type `assert_debug_absent` lists of
canary tests (Twilio `tests/canary.rs:116-118`; Europe PMC "success fixtures echo the query into
every withheld publication/reference text field", `europe-pmc-sdk/tests/canary.rs:9-10`), ≈ 10–40
lines per SDK, ≈ 800 overall (estimated). The comment headers that list withheld fields by hand
(`twilio-sdk/tests/canary.rs:1-9`) become code.

Hardest counterexample:
- validated deserializers (`#[serde(try_from = "String")]`: `europe-pmc-sdk/src/input.rs:8-9`,
  Salesforce ids) reject canaries: per-field overrides.
- S3's XML models (custom decoding in `amazon-s3-sdk/src/xml.rs`) are outside serde: not covered.
- native fields (`HeaderValue`) have no serde: such types use D2 and need no fill.
- twilio `Message`: `sid`, `status`, `error_code` are in `SHOWN`; their canaries may appear.

Simple control (Nager): no withheld model; unchanged.

Charter check:
- C9: the per-type claim becomes a one-line test that fails when a field is added unmarked — the
  test D1's denylist needs.

New concepts: `fill`, `fill_with`, `assert_withheld`; derive-emitted `SHOWN`.

Breaking changes: none.

Why it might be wrong: A deserializer that satisfies arbitrary serde types (flatten, untagged,
`deny_unknown_fields`) is clever code whose failures confuse authors (C6). It checks a type in
isolation; leaks through wrappers and error chains still need M2's HTTP-level canary.

---

# Across families

| Design | Family glue it removes (estimated) | Also removes / enables |
|---|---|---|
| D1 derive + `#[sensitive]` | D ≈ −1,250 of 2,954 Debug lines | A (Debug in `error.rs`), B (`Response<T>` Debug), C (wrapper-client Debug), F (cursor Debug) |
| D2 `Sensitive<T>` + `WireText` | D ≈ −450…550 | I (no `Display` for secrets), C (credential plumbing), F (opaque cursors); fact written once instead of per placement |
| D3 `#[public]` | as D1 | safe failure for evolving models; M5's `SHOWN` |
| D4 `Link` | D ≈ −120…150 | F (next links), J (origin-only logging), C (download hosts) |
| J1 static events | 0 SDK, ≈ 60 app | H and M3 share `EndpointMeta::route` |
| J2 `Summary` | ≈ 0 | A (error `Display` can delegate), C7 (redaction survives `into_parts`) |
| J3 call span | 0 SDK, ≈ 100 app | turns J1 events into attempts of one call |
| J4 `request_id` | ≈ 100 | A, B |
| M1 `concord-test` | tests ≈ −1,750 | base for M2–M5 |
| M2 conformance matrix | tests ≈ −1,300 | — |
| M3 `composition()` | tests ≈ −500 | H (doc line), J (route), K (query order visible) |
| M4 declared origin | tests: most of 352 `with_base_url`; `src/` ≈ 15 per SDK | C (a fixed base without a test-only constructor) |
| M5 canary fill | tests ≈ −800 | D1/D3's C9 |

Dependencies between designs: D3 is D1 plus one word; M5 needs D1 or D3; J3 and J4 build on J1;
M2–M5 live in M1's crate; M4 is what makes family C's "fixed base" testable. All designs leave the
three simple controls' declarations unchanged (rule C3).

Smallest set that removes most of the measured glue per family: **D1** (with D3's `#[public]` as
the open decision on the default), **J1 + J4**, **M1**.
