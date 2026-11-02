# Final record — native header values and body/response formats

Branch `concord-v4-native-repr`, worktree `/home/canarit/projects/concord_api_v4`. Started from
`a915c8d` (`concord-v4-refine`, code identical to `b0fe26e`). Nothing pushed, published or merged
into another branch. Untracked and untouched: `.claude/`, `concord_v4_handoff/`. Preserved:
`concord-v4-refine` (`a915c8d`), `concord-v4` (`8f7adc2`), V3 (`concord-v3`,
`concord-v3-preserved`), `concord-v4-alpha1-baseline`. Side branches merged here:
`native-repr-findings`, `native-repr-bench`, `native-repr-formats`.

**Candidate:** `1d9ffe7` (code). Later commits change only `v4/` records.

| | State |
|---|---|
| Implemented | native header values (N01–N05), formats (N06–N08), note deduplication (N09), `Clone` checks (N10), multipart recipe (N11), Criterion suite (N12), release preparation, doc fixes |
| Tested | `scripts/verify.sh --msrv --nightly --package`: 22/22 PASS on `1d9ffe7`, clean tree, Linux x86_64, rustc 1.97.1 / 1.85.1 / nightly 1.101; 537 workspace tests (`evidence/verify_1d9ffe7.txt`; earlier receipts `verify_66a4478.txt`, `verify_1174d5d.txt`, `verify_d591fe7.txt`, `verify_d6b209f.txt`) |
| Independently reviewed | Codex `gpt-6.1-sol` `xhigh`: blind design challenge, design pass 2, executable review of `66a4478` (REVISE: F1–F4), focused executable re-review of `1174d5d` (F1, F2, F4 resolved; F3a found), narrow checks of the migration fixes on `d591fe7` (F3a resolved; F3b–F3d found), `d6b209f` (F3–F3d resolved; F3e found) and `1d9ffe7` — see `REVIEW_TRIAGE.md` and the end of this record |
| Accepted | see "Acceptance" at the end |
| Windows / macOS | NOT_RUN for this candidate (a push was not authorized; CI run `41379941540` is of `a40788c`, baseline evidence only) / not a requirement |

## 1. A mostly-JSON API

Before (`a915c8d`):

```rust
use concord::{Binary, Json, Text};

concord::api! {
    pub client Shop(#[sensitive] header("authorization") token: String) {
        base "https://shop.example.test/v2";
        scope catalog {
            path "catalog";
            GET product(id: u64) -> Product via Json { path "products/{id}"; }
            POST create(body product: NewProduct via Json) -> Product via Json { path "products"; }
            PATCH patch(id: u64, body patch: ProductPatch via Json) -> Product via Json {
                path "products/{id}";
                headers { "content-type" = "application/merge-patch+json"; } // sent even without a body
            }
            DELETE delete(id: u64) { path "products/{id}"; }
        }
    }
}
```

After:

```rust
concord::api! {
    pub client Shop(#[sensitive] header("authorization") token: String) {
        base "https://shop.example.test/v2";
        request json;   // `body x: T` below is JSON
        response json;  // `-> T` below is decoded from JSON
        scope catalog {
            path "catalog";
            GET product(id: u64) -> Product { path "products/{id}"; }
            POST create(body product: NewProduct) -> Product { path "products"; }
            PATCH patch(id: u64, json("application/merge-patch+json") patch: ProductPatch) -> Product {
                path "products/{id}";
            }
            DELETE delete(id: u64) { path "products/{id}"; } // no `->`: any 2xx, drained (204 fine)
        }
    }
}
```

Calls and generated signatures are unchanged: the 102 `pub fn` of the 20-endpoint corpus were
byte-identical between the control and the owner-default prototype (and identical in the in-place
prototype), and the repository migration (`c112845..6b7fca0`) changed declarations, not call sites
— its only removed call lines belong to two runtime tests of a content-type check that moved to
compile time:

```rust
pub fn create(&self, product: impl IntoValue<NewProduct>) -> ShopCatalogCreate
pub fn prepare(self) -> Result<Prepared<Product>, Error>
pub async fn send(self) -> Result<Product, Error>
let created: Product = shop.catalog().create(NewProduct { .. }).send().await?;
```

Generated rustdoc of `create`: "Body (`product`): `NewProduct`, encoded with the format `json` (the
request default of the client `Shop`). Content-Type: `application/json` is sent with the body.
Output: `Product`, decoded from a 2xx response with the format `json` (the response default of the
client `Shop`)."

## 2. A mixed-format API

| Operation | Before | After |
|---|---|---|
| raw text out | `GET notes(id: u64) -> String via Text` | `GET notes(id: u64) -> text String` |
| JSON string out (under the JSON default) | `-> String via Json` | `-> String` |
| PNG upload | `PUT image(id: u64, body png: Bytes via Binary) { headers { "content-type" = "image/png"; } }` | `PUT image(id: u64, bytes("image/png") png: Bytes)` |
| bytes out | `-> Bytes via Binary` | `-> bytes Bytes` |
| form in, JSON out | `POST login(body f: Login via Form) -> Token via Json` (a downstream `Form` codec) | `POST login(form f: Login) -> Token` (feature `form`) |
| custom CSV out / in | `-> Vec<Row> via Csv`, `body rows: Vec<Row> via Csv` | `-> Csv Vec<Row>`, `Csv rows: Vec<Row>` |
| optional body (absent: no body, no `Content-Type`) | `body q?: Query via Json` | `body q?: Query` |
| JSON `null` body | `body d: Option<Discount> via Json` | `body d: Option<Discount>` |

Formats in place replace the default's format and media type; `Encode<T>`/`Decode<T>` are
unchanged for custom formats.

## 3. Native and static header values

Before: `header("accept") accept: HeaderValue` did not compile (three E0277: no `Display`, in the
placement and in the generated `Debug`); every header value was formatted, checked and copied per
request, and the native sensitivity flag would have been overwritten.

After:

```rust
use concord::reqwest::header::HeaderValue;

static ACCEPT_JSON: HeaderValue = HeaderValue::from_static("application/json");

concord::api! {
    pub client Shop(
        #[sensitive] header("authorization") token: String,
        header("accept") accept: HeaderValue = &ACCEPT_JSON, // shared bytes, no clone needed
    ) {
        base "https://shop.example.test/v2";
        request json;
        response json;
        DELETE delete(id: u64, header("x-signature") sig: HeaderValue) { path "products/{id}"; }
    }
}

let mut sig = HeaderValue::from_bytes(&signature_bytes)?; // any bytes `http` accepts
sig.set_sensitive(true);                                    // kept: never cleared by Concord
shop.delete(7, sig).send().await?;                          // flagged on the wire, redacted in Debug
```

Generated `set_accept(self, value: impl IntoValue<HeaderValue>) -> Self` accepts `&ACCEPT_JSON`
or a `HeaderValue`; rustdoc says "Initially `&ACCEPT_JSON` … As a header value it goes through
`concord::ToHeaderValue`: a native `HeaderValue` is sent byte for byte and keeps its sensitivity
flag; text must be visible ASCII …". Literal header values (`"accept" = "application/json"`) are
compile-time statics. Text, integers and `bool` work as before without `Display`; another
`Display` type placed directly as a header is a compile error naming the template or the trait.

## 4. Native multipart (`examples/multipart`)

Before: a hand-written MIME writer in `examples/native` (boundary search, quoted-string escaping).
After: reqwest's own multipart, enabled only in that example crate:

```rust
// The endpoint is declared without a body: `POST upload(folder: String) -> json Stored`.
pub async fn upload(api: &Files, folder: &str, form: Form) -> Result<Stored, concord::Error> {
    let (client, request, decoder) = api.upload(folder).prepare()?.into_parts();
    let result = reqwest::RequestBuilder::from_parts(client, request)
        .multipart(form) // reqwest sets the boundary `Content-Type` and the body
        .send()
        .await;
    decoder.finish(result).await // the declared 2xx check, JSON decoding, Concord's errors
}
```

## 5. Decisions (details: `DECISIONS.md`, `CONTRACT.md`)

- **N01** open trait `ToHeaderValue` with concrete impls; no `Display` fallback (E0119 proven on
  1.85 and 1.97.1); templates are the formatting route. Rejected: http's `TryFrom` (borrowed
  integers fail, Unicode text silently becomes obs-text), autoref dispatch, a `native` keyword, a
  `Format<T>` wrapper in caller signatures.
- **N02** outgoing flag = native flag OR Concord's classification, per value; never cleared. A value
  placed as a header and as text must be text itself (`String`, `&str`, integers, `bool`) unless
  `#[sensitive]` (review F1: a custom conversion's runtime flag cannot reach text placements).
- **N03–N05** visible ASCII for Rust text only; literal statics; native values are not text.
- **N06** one rule, two positions: formats in place or inherited from `request`/`response`
  owner defaults; no endpoint statements; `via` removed. Rejected: `via` (control), endpoint
  `request`/`response` statements, per-endpoint attributes, clearing defaults, output media types,
  rejecting `-> ()` under a default (spelling-dependent acceptance).
- **N07** media type belongs to the body; a surviving `content-type` header plus an explicit
  media type is a compile error; the inserted `Content-Type` carries the key's classification.
- **N08** built-in `form` behind a non-default feature.
- Accepted risk: under a JSON default a forgotten `-> text String` or `text note: String` is a
  runtime decode error (or a server rejection), not a compile error; rustdoc names every
  endpoint's format and its origin.

## 6. Previous review's localized findings

| Finding | Result |
|---|---|
| quadratic placement-note scan | linear (`01774a9`); fanout `check` −15% at 4,000 placements, 0.995 → 0.516 s at 16,000 |
| conditional `Clone` | strategy kept; `checks/clone-compat` positive/negative on 1.85, 1.97.1, nightly (non-blocking CI lane) |
| multipart recipe | reqwest multipart in `examples/multipart`; feature isolation checked in `verify.sh` and CI |
| arity advice, native `Debug` wording | corrected (`de957c3`) |
| fresh-use trial | README/quick start first, then rustdoc and guides on demand: 10/10 tasks, ~860 lines read, no source; fixes in `2c4ce3b` (`evidence/fresh_use/`) — an agent simulation, not a human study |
| release preparation | package ships only library sources and README; packaged-source consumer passes on stable, no-default and 1.85 (`scripts/package-check.sh`); rendered rustdoc inspected in a browser (bracketed links fixed) |
| base URL consistency test | kept unchanged |

## 7. Performance (`PERF.md`)

Runtime (Criterion, two candidate runs against the `a915c8d` baseline): request construction
with headers −7% to −15%, inherited composition −8% to −17%, text and numeric header values −5% to
−21%, the 9-query-value request ±2%; 16 native or literal header values 680–713 ns versus 845–877 ns
for the hand-written prebuilt equivalents; decoding and round trips within ±3%; small `bytes`/JSON
bodies +10–24 ns (`bytes` +8% to +13%, 100-byte JSON +1% to +5%), partly attributed (≈10 ns:
cloning the compile-time content-type static), the rest unattributed (no `perf`). Allocations per `prepare` fall (e.g. 16 text headers
43 → 27). Compile time (paired hyperfine): within ±4% on every row, declaring crate and consumers;
binaries within 0.12%, rlibs up to +1.15%. `#[inline]` unchanged.

## 8. Migration (`docs/MIGRATION.md`)

1. `scripts/migrate_via.py FILE...` moves `via R` before the payload and keeps the path `R`, so the
   result names the same types (`-> Item via Json` → `-> Json Item`); a bare lowercase name is left
   for the author (`self::name`). A leftover `via` is a compile error naming the same path.
   Writing Concord's own markers as the words `json`/`text`/`bytes` is a manual, optional step.
2. Optionally hoist a shared format to `request json; response json;` on the client or uniform
   scopes, and write `body x: T` / `-> T`.
3. Move static `content-type` headers to the body: `json("application/merge-patch+json") p: T`.
4. Header values of other `Display` types: a template (`"x-k" = "{v}"`) or `impl ToHeaderValue`.
5. A downstream `Form` codec can become `form x: T` with feature `form`.

## 9. Limitations

- Forgetting a format exception under a default is a runtime error (accepted risk, N06).
- rustc's own wording appears in two diagnostics (`$crate` in the feature-off error; `E0080` for an
  invalid `Encode::CONTENT_TYPE`).
- A `///` comment directly before an owner statement is still rejected (pre-existing parser wart).
- The ~8 ns small-body cost is unattributed; HTTP/2/TLS, allocation counts in round trips and
  profiles were not measured.
- Windows and macOS were not executed for this candidate.
- Agent trials and reviews are simulations; no human study was run.

## Acceptance

**Accepted for Linux x86_64** (rustc 1.97.1; MSRV 1.85.1; nightly as a non-blocking lane) at code
revision `1d9ffe7`:

- every charter requirement in §1–§6 above is implemented and covered by executable tests
  (`verify.sh --msrv --nightly --package` 22/22 on a clean tree, `evidence/verify_1d9ffe7.txt`);
- every finding of the independent executable reviews is fixed and was confirmed resolved by a
  later executable Codex review (F1, F2, F4 on `1174d5d`; F3 and F3a–F3d on `d6b209f`; F3e on
  `1d9ffe7`); the design-review findings are dispositioned (`REVIEW_TRIAGE.md`);
- performance is measured against the `a915c8d` baseline, with one documented cost (`PERF.md`).

Scope of the independent review: the full executable audit ran on `66a4478`; the changes made
since (the dual-use header rule, the package check, the migration script and hint, an absolute
path in the parser, records) were each reviewed by focused or narrow executable Codex checks of
those areas, not by a second full audit. Implemented, tested, reviewed and accepted are therefore
distinct for the final delta only in that sense.

**Not established:** Windows (NOT_RUN for this candidate: a push was not authorized), macOS
(NOT_RUN), human usability (agent simulations only), HTTP/2 and TLS performance, universal safety
of any kind beyond the tested properties.
