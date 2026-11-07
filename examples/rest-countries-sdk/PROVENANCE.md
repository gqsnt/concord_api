# Provenance

Where every provider fact, fixture and dependency of `concord-sdk-rest-countries` comes from.
Live provider calls: **NOT_RUN**. No request was ever sent to `api.restcountries.com`; every test
runs against `concord-example-testserver` on 127.0.0.1.

## Documentation read

Both pages were read on **2026-11-07** (around 06:55 UTC): fetched as HTML with `curl` (copies
kept only under the worktree's `target/tmp/docs/`, not part of the package) and read as text. The
pages' "Run →" buttons point at the live API; none was followed.

| Page | Sections used |
|---|---|
| <https://restcountries.com/docs/countries> | Overview (read-only, `GET` only, `Content-Type: application/json` on every response, `data` / `errors` shapes), Try it now (demo key returns a sample plus a `data._demo` block), Quick start (`Authorization: Bearer`; a lookup returns "a single-record `data.objects` array"), Base URL & versioning (`https://api.restcountries.com/countries/v5`), List, Search, Aggregate, Read by property, Search by property (paths, parameters, examples), Field reference (property paths, types, *no read* / *no search* / *premium* flags), Premium Data (in-place notice, `403` document), Sort order, Pagination (`data.meta`, out-of-range offsets), Errors (status table, `errors[].message`), Gotchas (CORS) |
| <https://restcountries.com/docs/countries/api-versions> | `/v5` maintained, `/v1`–`/v4` deprecated; v5 served from `api.restcountries.com`; `Authorization: Bearer` preferred, `?api-key=` fallback "for browser experiments", the bare `Authorization: KEY` form also accepted |

## Access terms (contract note: "verify current access terms before use")

What the two pages say, as of 2026-11-07:

- v5 requires an API key (account sign-up). A demo key exists that "returns a real,
  correctly-shaped sample" with a `data._demo` notice; this crate surfaces that block as
  `CountryPage::demo` so sample data is not mistaken for the dataset.
- Free plan: `limit` 1–100; paid plans up to 500. Premium properties (`leaders`) need a Personal
  plan or above. An account is frozen (`403`) when its monthly request limit is exceeded; the
  monthly quota itself is not stated on these pages.
- An edge rate limiter answers `429` above 20 requests per 10 seconds.
- Browser use needs allowed origins on the key; server-side calls are unaffected.

Not verified: the provider's Terms of Service, Acceptable use, plan and pricing pages. They are
outside the two pages this slice may read, so their current terms were **not** checked. Anyone
running this SDK against the real service must read them first.

## Provider facts and where the code uses them

| Fact (source section) | Code |
|---|---|
| Base `https://api.restcountries.com/countries/v5` (Base URL & versioning) | `base` + client `path "countries/v5"` in `src/lib.rs` (the prefix survives `with_base_url`) |
| `Authorization: Bearer YOUR_API_KEY` preferred; `?api-key=` fallback (api-versions) | `bearer()` and the `#[sensitive] header(AUTHORIZATION)` client value; the query form is never sent (`assert_get` checks) |
| All endpoints `GET`, JSON, success under `data`, failures under `errors[].message` (Overview, Errors) | `format Json;`, `CountryPage` envelope, `ErrorDocument`, `SendPolicy` |
| List `GET /countries/v5` with `limit`, `offset`, `q`, `{property}` filters (AND), `response_fields`, `response_fields_omit` (List, Search) | endpoint `countries`, `Filters` (`query(..)`), `params ListOptions` |
| Aggregates `name` (common, official, alternates, native; not translations) and `code` (every `codes.*`) (Aggregate) | endpoints `search_names`, `search_codes` |
| `GET /countries/v5/{property}/{value}`: exact, case-insensitive; optional `q` (Read by property) | scope `property`, endpoint `read`; `RestCountries::country` |
| `GET /countries/v5/{property}?q=`: substring within one property (Search by property) | scope `property`, endpoint `search` |
| Property paths, their types, *no read* / *no search* / *premium* (Field reference) | `Property` constants and grammar, `Country` field types |
| Premium value replaced in place by `[{"message": ..}]`; premium search/filter/lookup is `403` (Premium Data) | `Premium::Withheld`; `ProviderClass::Forbidden` |
| `data.meta` `{total, count, limit, offset, more}` on every endpoint returning objects; out-of-range offset is an empty `200` page (Pagination) | `PageMeta` (required), `CountryPage::next_offset`, `pages()` |
| Status table 400/401/403/404/405/410/429/500 (Errors) | `ProviderClass` |
| `limit` above the plan ceiling or below 1 is an error (Pagination) | `limit` is a `NonZeroU32`; the plan ceiling stays the provider's (`400`) |
| `+` and `%20` both decode as spaces (Search, Read by property) | Concord's encodings (`+` in the query, `%20` in a path segment) are both accepted |

## Inferences (not documented; not verifiable without live calls)

1. **No-match shape.** No page shows a search or lookup without match. The SDK expects the
   documented page shape with `objects: []`, `total: 0`, `more: false` (consistent with the
   documented empty `200` for an out-of-range offset, and with `404` being reserved for unknown
   paths and non-readable/non-searchable properties). A `404` for an unmatched *value* would
   surface as `Error::Provider` (`NotFound`), not as `None`.
2. **Comma in `response_fields`.** The examples write raw commas; Concord's query encoding sends
   `%2C`. A conforming form/URL query decoder reads both as `,`; whether the provider does was not
   verified (FRICTION.md, item 1).
3. **Code uniqueness.** ISO alpha-2/alpha-3 lookups are expected to match at most one record
   ("single-record" in Quick start); more is reported as `ContractError::NotUnique`.
4. **Undocumented shapes kept as JSON:** the `_demo` block, `leaders` entries, `flag.colors`,
   capital `coordinates`, and every property this crate does not type (`Country::other`).
5. **Boolean filter spelling.** The list examples spell a boolean filter `memberships.eu=1`; the
   path lookups spell it `true`/`false`. `Filters` sends the caller's text and does not choose.

Observed documentation inconsistency: the Aggregate (`Canada, Cape Verde, Cameroon`) and Search
by property (`Germany, Niger, Nigeria, Algeria`) examples are not in ascending `names.common`
order, although Sort order says multi-object responses are. The SDK never relies on the order.

## Fixtures

`doc-*` files are copied verbatim from the documentation examples (JSON reformatting only where
noted). `derived-*` files embed documented text in an authored envelope. `synthetic-*` files are
authored for this crate; their values are not provider data and were not checked against the
service. Expectations in the tests are written from the documentation or from the fixture text,
never from the SDK's output.

| Fixture | Origin | Derivation | Used by |
|---|---|---|---|
| `doc-list-fields-limit3.json` | Countries page, List → Example → Response | verbatim | protocol: documented response examples |
| `doc-search-germ-europe.json` | Search → Example → Response | verbatim | protocol: documented response examples |
| `doc-name-can.json` | Aggregate → Example → Response | verbatim | protocol: documented response examples |
| `doc-capitals-tokyo.json` | Read by property → Example → Response | verbatim | protocol: documented response examples |
| `doc-names-common-ger.json` | Search by property → Example → Response | verbatim | protocol: documented response examples |
| `doc-error-400-limit.json` | Errors → example body | verbatim | protocol: error classes; workflows (fallback route) |
| `doc-error-403-premium.json` | Premium Data → filter/search error body | verbatim | protocol: error classes; workflow 2 premium filter |
| `derived-out-of-range-offset.json` | Pagination → `data.meta` example | `objects: [ ... ]` replaced by `[]`, `count` 0, `offset` 300 (> `total` 249), `more` false: the documented "out-of-bounds offsets return an empty objects array (status 200)" | workflow 2: offset past the end |
| `derived-leaders-free-plan.json` | Premium Data → in-place notice | the documented `leaders` notice inside an authored one-record page (`France`, `names.common` only) | workflow 2: unavailable selected field |
| `synthetic-no-match.json` | authored | inferred no-match page (Inference 1) | protocol: request shapes; workflow 1: no match |
| `synthetic-name-germ.json` | authored | name-search candidates with `names.common`, `names.official`, `codes.alpha_3` | workflow 1 |
| `synthetic-germany-deu.json` | authored | a fuller record using documented property names; includes the documented premium notice and an invented `a_future_v5_field` for forward compatibility; `flag.url_svg` points at `example.invalid` | workflow 1; protocol: unknown fields |
| `synthetic-europe-eu-page1.json`, `synthetic-europe-eu-page2.json` | authored | a 3-record matched set paged with `limit` 2 | workflow 2: filter, fields, walk |
| `synthetic-parent-absent.json` | authored | one record without the selected `parent.alpha_2`, one with it | workflow 2: unavailable selected field |
| `synthetic-demo-sample.json` | authored | a page with an invented `_demo` block (its real shape is undocumented) | protocol: demo notice |
| `synthetic-not-unique.json` | authored | two records for one code lookup | workflow 1: ambiguous lookup |
| `synthetic-more-mismatch.json`, `synthetic-count-mismatch.json` | authored | self-contradictory `data.meta` | workflow 2: contract errors |
| `synthetic-errors-on-200.json`, `synthetic-missing-meta.json`, `synthetic-wrong-types.json`, `synthetic-truncated.json`, `synthetic-malformed.json` | authored | invalid success bodies | protocol: decode failures |
| `synthetic-proxy-502.html` | authored | an HTML page from an intermediary | protocol: wrong media |
| `synthetic-error-401.json`, `-403-frozen.json`, `-404.json`, `-405.json`, `-410.json`, `-429.json`, `-500.json` | authored | the documented `errors[].message` shape with invented messages (only the 400 and premium 403 messages are documented) | protocol: error classes; workflow 2 (404) |
| `synthetic-error-wrong-shape.json` | authored | a JSON error body that is not `errors[]` | protocol: unreadable error documents |

Tests also build a few bodies inline, each labelled where it is written: a 401 document echoing
the synthetic key (redaction), an 80 KiB error document (bound), and a lazily generated 64 MiB
success body (bound).

Every `pages()` walk in the tests sets `max_pages`. Two mutants (a dropped `count` check, and a
walk that ignores the computed offset) first made two uncapped tests loop on a fixture the
server repeats. They were then capped, so such a defect fails the test instead of hanging it.

## Dependencies

No new crate: `Cargo.lock` gains only the `concord-sdk-rest-countries` entry, and no other
package changed version.

| Crate | Why |
|---|---|
| `concord` (workspace, feature `json`) | the declaration, `Json`, the send policy |
| `concord-auth` (workspace) | `bearer()`: a validated, sensitive `Authorization` value; its `base64` 0.23.1 was already in the lock |
| `concord-pagination` (workspace) | `next_offset` (checked page arithmetic) and `Pages` (the bounded walk of `pages()`) |
| `serde`, `serde_json` (workspace) | the models; `serde_json::Value`/`Number` for undocumented shapes and exact numbers |
| `concord-example-testserver`, `tokio` (dev, workspace) | the offline tests |

## Examples consulted

Example-assisted: `examples/frankfurter-sdk` only (another small public-data SDK with a send
policy). Files read and what was taken:

- `Cargo.toml`: the package layout (workspace lints, testserver dev-dependency).
- `src/lib.rs`: the `send SendPolicy` pattern — non-2xx read with
  `received.document::<Json, _>(LIMIT)`, success with `decode`.
- `src/error.rs`: the `ErrorBodyError` split by `ErrorKind` (Metadata / Read / Decode), and a
  `ProviderError` whose `Debug` hides the document's text.
- `src/model.rs`: skimmed; nothing taken (decimal handling is Frankfurter-specific).
- `tests/common/mod.rs`: an offline `reqwest::Client` (no proxy, no redirects, timeout, fixed user
  agent) and a shared `assert_wire` helper, adapted as `tests/support/mod.rs`.
- `tests/protocol.rs` (first 200 lines): the per-case `TestServer` + `shutdown().open_after == 0`
  test shape.

No other example SDK was read. These were read as documentation:

- Concord's own documents: `README.md`, `docs/QUICKSTART.md`, `docs/AUTHOR_GUIDE.md`,
  `docs/CHEATSHEET.md` and `docs/RECIPES.md`.
- The READMEs of `concord-auth`, `concord-pagination`, `concord-retry`, `concord-ratelimit` and
  `concord-codecs`.
- `examples/testserver/src/lib.rs`.
- The public items (signatures and their docs) of the crates used: `concord-core`'s
  `prepared.rs`, `response.rs`, `value.rs` and `header.rs`, `concord-pagination`'s `pages.rs` and
  `offset.rs`, and `concord-auth`'s `credentials.rs`.
