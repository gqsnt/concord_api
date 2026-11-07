# concord-sdk-rest-countries

A bounded Rust SDK for the [REST Countries](https://restcountries.com) **Countries API, version
5** (`https://api.restcountries.com/countries/v5`), built on Concord. It covers two connected
workflows:

1. **Search country names and fetch one chosen country by its ISO code.**
2. **Filter countries by documented properties and select the response fields.**

Status: an example SDK of this workspace (`publish = false`). Every test runs offline against
local fixtures. **No request was ever made to the real service** (live calls: NOT_RUN). The
provider requires an API key. Its plans, quotas and Terms of Service were not reviewed for this
crate (PROVENANCE.md, "Access terms"), so check them before real use.

## Create a client

```rust,no_run
use concord_sdk_rest_countries::concord::reqwest;
use concord_sdk_rest_countries::{RestCountries, RestCountriesConfig, bearer};

fn client(api_key: &str) -> Result<RestCountries, Box<dyn std::error::Error>> {
    // Transport settings (timeouts, proxy, redirects) belong to your reqwest::Client.
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    // `bearer` refuses an empty key, whitespace, control characters and non-ASCII bytes.
    Ok(RestCountries::new(http, RestCountriesConfig { authorization: bearer(api_key)? }))
}
```

The key travels as `Authorization: Bearer <key>`, the form the provider prefers. It is never put
in the URL; the `?api-key=` fallback is not used. It is withheld from every `Debug` and
`Display` of the client, its requests and this crate's errors.

## Workflow 1: search names, fetch the chosen country by code

```rust,no_run
use concord_sdk_rest_countries::{CountryCode, Fields, Property, RestCountries};

async fn germany(api: &RestCountries) -> Result<(), Box<dyn std::error::Error>> {
    // `GET /countries/v5/name?q=germ&response_fields=names.common,codes.alpha_3`
    let candidates = api
        .search_names("germ")
        .set_response_fields(Fields::new([Property::NAMES_COMMON, Property::CODES_ALPHA_3])?)
        .send()
        .await?;
    let Some(chosen) = candidates.countries.iter().find(|c| c.common_name() == Some("Germany"))
    else {
        return Ok(()); // no match: an empty page, not an error
    };

    // `GET /countries/v5/codes.alpha_3/DEU`
    let code = CountryCode::parse(chosen.alpha_3().ok_or("codes.alpha_3 was not selected")?)?;
    match api.country(&code).send().await?.single()? {
        Some(country) => println!("{:?} borders {:?}", country.common_name(), country.borders),
        None => println!("no country has the code {code}"),
    }
    Ok(())
}
```

## Workflow 2: filter by property, select fields

```rust,no_run
use std::num::NonZeroUsize;

use concord_sdk_rest_countries::{Fields, Filters, Property, RestCountries, pages};

async fn eu_members(api: &RestCountries) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    // `GET /countries/v5?region=Europe&memberships.eu=1&response_fields=names.common,codes.alpha_2`
    let filters = Filters::new()
        .with(Property::REGION, "Europe")?
        .with(Property::MEMBERSHIPS_EU, "1")?;
    let fields = Fields::new([Property::NAMES_COMMON, Property::CODES_ALPHA_2])?;
    let listing = api.countries().set_filters(filters).set_response_fields(fields);

    // One page (25 records unless `set_limit`), with the provider's description of the slice...
    let first = listing.clone().send().await?;
    println!("{} of {} matches, more: {}", first.meta.count, first.meta.total, first.meta.more);

    // ...or every page, explicitly and bounded.
    let all = pages(listing)
        .max_pages(NonZeroUsize::new(20).unwrap())
        .items()
        .collect_complete()
        .await?;
    Ok(all.iter().filter_map(|c| c.common_name()).map(str::to_owned).collect())
}
```

## What is declared

| Call | Request |
|---|---|
| `api.countries()` (+ `set_q`, `set_filters`) | `GET /countries/v5`: every country, narrowed by free text `q` and property filters (AND) |
| `api.search_names(q)` | `GET /countries/v5/name?q=`: common, official, alternate and native names (not translations) |
| `api.search_codes(q)` | `GET /countries/v5/code?q=`: every `codes.*` variant |
| `api.property(p).read(value)` (+ `set_q`) | `GET /countries/v5/{property}/{value}`: exact, case-insensitive match (array properties: contains) |
| `api.property(p).search(q)` | `GET /countries/v5/{property}?q=`: substring within one property |
| `api.country(&code)` | `property(codes.alpha_2 / codes.alpha_3).read(code)` |

Every call accepts `set_response_fields(Fields)` (allowlist), `set_response_fields_omit(Fields)`
(blocklist, applied after the allowlist), `set_limit(NonZeroU32)` and `set_offset(u64)`, and
returns a `CountryPage`: `countries` (`data.objects`), `meta` (`total`, `count`, `limit`,
`offset`, `more`, `request_id`) and `demo` (the provider's `_demo` notice when the demo key
was used, meaning the records are a sample).

Inputs carry the provider's rules and are checked before anything is sent:

- `Property`: a dot-path of `a-z`, `0-9` and `_` (`region`, `names.common`, `memberships.eu`).
  Constants cover the documented properties these workflows use; `Property::new` takes any
  other.
- `Fields`: a non-empty list, sent comma-separated.
- `Filters`: `{property}={value}` pairs; the endpoint's own parameter names (`q`, `limit`,
  `offset`, `response_fields`, `response_fields_omit`, `pretty`) are refused as filter keys.
- `CountryCode`: two or three ASCII letters (ISO 3166-1 alpha-2 / alpha-3), uppercased.

`Country` types the documented properties used here (`names`, `codes`, `flag`, `capitals`,
`region`, `subregion`, `continents`, `landlocked`, `borders`, `population`, `memberships`,
`leaders`). Every one is optional. Every other top-level property the provider sends stays
available, unchanged, in `Country::other`.

## Failure semantics

Every `send()` makes **one** request. It never retries, never requests another page and never
touches the credential.

| Outcome | Result |
|---|---|
| A search or lookup without match | `Ok(CountryPage)` with no records (`meta.total` 0); `single()` gives `Ok(None)`; `pages()` is complete after that one request |
| An offset past the end | `Ok` with an empty page (documented); `next_offset()` is `None` |
| A selected property a record lacks | that field is `None` (or absent from `other`) |
| A premium property selected on a free plan | `Premium::Withheld { message }`: the provider's notice, not a failure |
| Any non-2xx status | `Error::Provider`: `status()`, `class()` (`BadRequest` 400, `Unauthorized` 401, `Forbidden` 403, `NotFound` 404, `MethodNotAllowed` 405, `Gone` 410, `RateLimited` 429, `Server` 5xx, `Undocumented`), `messages()` from the `errors[].message` document read up to 64 KiB, and `meta` (all response headers). An unreadable document keeps the status (`document: Err(ErrorBodyError)`) |
| Filtering, searching or reading by a property that is not searchable/readable | `Error::Provider` with `NotFound` (for example `population`, `flag.emoji`) |
| Filtering by a premium property on a free plan, or a frozen account | `Error::Provider` with `Forbidden`; the message says which |
| A success that is not JSON-labelled, malformed, truncated or over 16 MiB | `Error::Concord` (`ErrorKind::Metadata`, `Decode`, `Transport` or `Limit`), with the response head when one arrived |
| A 2xx page that contradicts itself (`count`, `more`, offsets), or a code lookup matching several records | `Error::Contract` / `ContractError` (from `next_offset()`, `single()`, `pages()`) |
| An input that breaks a rule above | `InputError` or `CredentialError`, before any request |

Provider messages are untrusted text. `Debug` and `Display` of `Error` show the status and class,
never the messages; read them with `messages()`.

`prepare()` and `with_base_url` return `concord::Error` (re-exported as `concord`); rustdoc shows
it as `Error` too. `?` converts it into this crate's `Error`.

## Transport notes

- reqwest drops `Authorization` when a redirect changes host, port or scheme. The provider
  documents no redirects; a client with `redirect::Policy::none()` makes every answer a single,
  accountable response. A redirect then surfaces as `Error::Provider` (`Undocumented`).
- The edge rate limiter answers `429` above 20 requests per 10 seconds per account. Pacing is the
  caller's (see FRICTION.md for why no limiter is built in).

## Excluded

- The provider's other products: the User Agent, IP, Email Intelligence and Currencies APIs,
  webhooks, the Flag, Favicon and Brand CDNs, and the Business-plan EU-region host.
- Offline mirroring, historical datasets, and any v1–v4 behaviour (including inferred v3
  compatibility). Only v5 is declared.
- `?api-key=` query authentication, `pretty` output, browser/CORS concerns.
- Automatic retries, rate admission and implicit pagination.

## Tests

```sh
cargo test --locked -p concord-sdk-rest-countries
cargo clippy --locked -p concord-sdk-rest-countries --all-targets -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-sdk-rest-countries --no-deps
```

`tests/protocol.rs` covers the wire shapes against the documented examples, every documented
error class, malformed, truncated, oversized and wrong-media bodies, and key redaction.
`tests/workflows.rs` runs both workflows end to end, including the required failure: a no-match
and unavailable fields stay ordinary outcomes. Fixture origins are in PROVENANCE.md, Concord
costs in FRICTION.md.
