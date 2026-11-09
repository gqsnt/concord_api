# concord-sdk-cloudflare-zones

A bounded slice of the Cloudflare API v4 on [Concord](../../README.md): list the zones
of an account and fetch one zone; list, fetch and create a zone's DNS records. It is tested
offline against local fixtures only: **no request has been made to the real API** (live status
NOT_RUN), so agreement with the service rests on the cited documentation
([`PROVENANCE.md`](PROVENANCE.md)).

| Call | Request |
|---|---|
| `cf.zones().list()` | `GET /client/v4/zones` (List Zones) |
| `cf.zone(zone_id).get()` | `GET /client/v4/zones/{zone_id}` (Zone Details) |
| `cf.zone(zone_id).dns_records().list()` | `GET /client/v4/zones/{zone_id}/dns_records` (List DNS Records) |
| `cf.zone(zone_id).dns_record(record_id).get()` | `GET /client/v4/zones/{zone_id}/dns_records/{record_id}` (DNS Record Details) |
| `cf.zone(zone_id).dns_records().create(record)` | `POST /client/v4/zones/{zone_id}/dns_records` (Create DNS Record) |

## Calling it

`Cloudflare::connect` takes your `reqwest::Client` and an API token, sent as
`Authorization: Bearer {token}`. Navigation and setters are Concord's generated ones; every
`send()` returns the SDK's result. The listings also walk their pages with `.pages()`.

```rust,no_run
use concord_sdk_cloudflare_zones::{
    AccountId, Cloudflare, RecordType, RecordsPerPage, ZonesPerPage, http_client,
};

async fn run(api_token: &str) -> Result<(), Box<dyn std::error::Error>> {
    let cf = Cloudflare::connect(http_client()?, api_token)?;

    // Workflow 1: every zone of an account, page by page, then one zone by its identifier.
    let zones = cf
        .zones()
        .list()
        .set_account_id(AccountId::from("0123456789abcdef0123456789abcdef"))
        .set_per_page(ZonesPerPage::MAX)
        .pages()
        .items()
        .collect_complete() // `Ok` only after the last page; otherwise the prefix and the error
        .await?;
    let zone = cf.zone(zones[0].id.clone()).get().send().await?.result;
    println!("{} is {:?}", zone.name, zone.status);

    // Workflow 2: the zone's DNS records, then one record.
    let records = cf
        .zone(zone.id.clone())
        .dns_records()
        .list()
        .set_record_type(RecordType::A)
        .set_per_page(RecordsPerPage::new(500).unwrap())
        .pages()
        .items()
        .collect_complete()
        .await?;
    if let Some(first) = records.first() {
        let record = cf.zone(zone.id).dns_record(first.id.clone()).get().send().await?;
        println!("{} {} {:?}", record.result.name, record.result.ttl, record.result.content);
    }
    Ok(())
}
```

What `send()` returns (the generated rustdoc shows it as
`<SendPolicy as SendWith<Single<Zone>>>::Output`, see `FRICTION.md` F1):

| Endpoint | `send()` |
|---|---|
| `get()` (zone, record), `create(record)` | `Result<Response<T>, Error>`: `result: T`, `messages`, `errors` (non-fatal, see below), `meta` |
| `list()` (zones, records) | `Result<PageResponse<T>, Error>`: `result: Vec<T>`, `result_info: Option<PageInfo>`, `messages`, `errors`, `meta` |
| `list().pages()` | `ZonePages` / `RecordPages` (`concord_pagination::Pages`): `next_page()`, `items()`, `collect_complete()`, `max_pages(..)`, `finish()` |

Identifiers are typed (`ZoneId`, `RecordId`, `AccountId`): a value read from a response is passed
as is (`zone.id.clone()`), a literal with `ZoneId::from("…")`. Each is sent as exactly one path
segment (`a/b` becomes `a%2Fb`).

`prepare()` builds the native request without the send policy, and `prepare()?.send()` returns
the wire envelope (`wire::Single`, `wire::Paged`), a refusal included, for callers that want it.
For local fixtures, use `.with_base_url(server.url())`: the declaration appends `/client/v4`.

## Creating a DNS record

```rust,no_run
use concord_sdk_cloudflare_zones::{
    Cloudflare, Error, NewDnsRecord, RecordTtl, RecordType, ZoneId,
};

async fn create(cf: &Cloudflare, zone: ZoneId) -> Result<(), Error> {
    let body = NewDnsRecord::new(
        RecordType::A, "new.example.com", "192.0.2.42",
        RecordTtl::AUTOMATIC, Some(true),
    )?;
    match cf.zone(zone.clone()).dns_records().create(body).send().await {
        Ok(created) => println!("created {}", created.result.id),
        Err(Error::CreateUncertain(cause)) => {
            // The write may have committed. Fetch every page and inspect matching records.
            let listing = cf.zone(zone).dns_records().list()
                .set_record_type(RecordType::A)
                .pages().items().collect_complete().await;
            match listing {
                Ok(records) => {
                    for record in records.iter().filter(|r| r.name == "new.example.com"
                        && r.content.as_deref() == Some("192.0.2.42")) {
                        println!("reconciliation candidate: {}", record.id);
                    }
                }
                Err(incomplete) => eprintln!("reconciliation listing incomplete: {incomplete:?}"),
            }
            // The application decides how to handle candidates; preserve uncertainty here.
            return Err(Error::CreateUncertain(cause));
        }
        Err(other) => return Err(other),
    }
    Ok(())
}
```

Create supports **A, AAAA, CNAME, NS, PTR and TXT**. `NewDnsRecord::new` returns
`RecordInputError`; `?` converts it to `Error::Input`. Fields are private and have no unchecked
deserializer. Names must be complete ASCII/Punycode DNS names of 1–255 bytes. Address content
must match the IP family, CNAME targets must differ from the owner name, and NS/PTR targets
must be domain names. TXT uses quoted character strings; Cloudflare splits long strings.
`proxied: true` is allowed for A/AAAA/CNAME; `None` omits the field and `Some(false)` sends false.
These rules follow the [create reference](https://developers.cloudflare.com/api/resources/dns/subresources/records/methods/create/),
read **2026-11-07**, with conservative SDK restrictions recorded in `PROVENANCE.md`.

`RecordTtl::new` accepts `1` or 60–86400 seconds. `RecordTtl::enterprise` accepts `1` or
30–86400; calling it asserts that the selected zone is Enterprise. The create send policy
checks the zone ID's 32-character maximum before sending. Empty/dot path segments are refused
by Concord as `Error::Concord`/`Build`; overlong IDs are `Error::Input`. Cross-record conflicts,
permissions and zone-plan entitlement are provider responsibilities. The token needs `DNS Write`.

Create is sent **once**, including when the opt-in read retry policy is enabled and the response
is 429. `Error::CreateUncertain` covers transport failures, including timeouts before headers or
during a successful body read, and successful bodies that fail decoding/media/size checks.
List all pages to reconcile by type, name and content; an absent or ambiguous match does not
establish that resubmission is safe. A missing/null successful `result` remains `Error::Envelope`
and also needs reconciliation. Use `http_client()` or a custom client with retries and redirects
disabled. Native `prepare()?.send()` bypasses SDK target validation and error classification;
it is the caller's transport contract.

## Failure semantics

Every failure is a `concord_sdk_cloudflare_zones::Error`:

| Situation | Error |
|---|---|
| HTTP 2xx whose envelope says `success: false` | `Error::Provider`: `status()` is the 2xx, `errors()` and `messages()` are Cloudflare's, in order. **Never an empty result**, also when `result` has an unexpected shape. |
| A non-2xx status other than 429 | `Error::Provider` with the status. `document()` is the error envelope, or why it could not be read (HTML, empty, malformed, over 64 KiB: `ErrorKind::Metadata`, `Decode`, `Limit`); the status is authoritative either way. |
| HTTP 429 | `Error::RateLimited`: `retry_after()` (`RetryAfter::After(d)`, `Absent`, `Invalid`), `meta().rate_limits()` |
| 2xx `success: true` without `result` | `Error::Envelope` (`EnvelopeProblem::MissingResult`) |
| A page walk on a page without `result_info.page`/`total_pages`, or answering another page | `Error::Envelope` (`MissingPageInfo`, `PageMismatch`) |
| 2xx that is not `application/json` | `Error::Concord`, `ErrorKind::Metadata`, the response unread |
| A body that is not an envelope (malformed JSON, missing `success`, a malformed `result` beside `success: true`) | `Error::Concord`, `ErrorKind::Decode` |
| A body cut off | `Error::Concord`, `ErrorKind::Transport` (with the head) |
| A success body over 16 MiB (`SUCCESS_BODY_LIMIT`) | `Error::Concord`, `ErrorKind::Limit` |
| No response | `Error::Concord`, `ErrorKind::Transport`, `status()` is `None` |
| An invalid token at `connect`; an identifier that cannot be a path segment (`..`) | `Error::Credential` / `Error::Concord` `ErrorKind::Build`; nothing is sent |
| Invalid create input | `RecordInputError` / `Error::Input`; nothing is sent |
| Create transport failure or unusable successful body | `Error::CreateUncertain`; inspect the underlying Concord diagnostic, list records to reconcile, never automatically resend |

The existing `Error::Concord` transport/decode/media/size rows describe reads; create uses the
uncertainty rule above. Provider refusals (including `success: false` on 2xx) and 429 retain
their typed error classes for create as well.

`success` is authoritative: the documented 200 examples carry an `errors` entry beside
`success: true`, so such entries are returned in `Response::errors` and do not fail the call.

A page walk stops at the first failed page with `PagesError::Provider { error, failed, .. }`;
`collect_complete()` returns the pages read before it (`Incomplete::collected`), and
`list.pages().starting_at(failed)` resumes. The walk continues while `page < total_pages`; `total_count` is
documented as the count *without* search parameters and is never used to decide completeness.
Without `max_pages` there is no cap; a cap reached while pages remain is `PagesError::Bounded`,
never a complete result. A listing that changes between requests can repeat or skip items.

For `.pages().starting_at(from)` on either listing, use `concord_sdk_cloudflare_zones::concord_pagination::Resume`
for the position. `Resume::Start` repeats page 1 of the zones or DNS records listing, replacing
the template's page; `Resume::At(key)` takes the `NonZeroU32` page number in a returned page's
`next`.

## Rate limits

Cloudflare applies a global limit (per user or account token, and per IP) and per-service limits
to the same requests. The SDK does not throttle on its own and assumes no client-side bucket:

- `rate_limits()` on the `ResponseMeta` of every `Response`, `PageResponse`, walked page
  (`PageMeta::meta`), `ProviderError` and `EnvelopeError` returns every `Ratelimit` item (name,
  remaining `r`, reset `t`) and `Ratelimit-Policy` item (name, quota `q`, window `w`) by name and
  in order; `exhausted()` lists those at zero. A malformed header is an error of that accessor
  only, never of the call.
- A 429 is `Error::RateLimited` with its `Retry-After`. The default `SendPolicy` sends each request
  once. Opt in to waiting with
  The declared `mut policy` permits
  `cf.set_policy(SendPolicy::default().retry_rate_limited(attempts, max_retry_after))`: a `GET`
  answered 429 with a valid `Retry-After` of at most `max_retry_after` is sent again after exactly
  that delay, at most `attempts` times in all. Nothing else is repeated (no 5xx, no transport
  failure, no `success: false`), and no local backoff is added.

`http_client()` builds a transport without reqwest retries or redirects and with a 30-second
timeout; any `reqwest::Client` works, but its own retries would add requests the policy does not
count.

## Secrets

The token is a sensitive `HeaderValue` (`concord_auth::bearer`) and a `#[sensitive]` client value:
it never appears in the `Debug` of the client, scopes, requests or prepared requests, nor in any
`Error` `Display` or `Debug`. `ResponseMeta`'s `Debug` prints header names only (Cloudflare sets
cookies). `Display` of a provider error carries the status, the operation and the error codes;
Cloudflare's message texts are read through `errors()`/`messages()` and do appear in `Debug`.

## Excluded

Zone mutation and DNS record edit, delete, import, export, batch and scan, Workers, WAF,
GraphQL, the legacy `X-Auth-Email`/`X-Auth-Key` scheme, and any global token-bucket assumption.
Not implemented on the two listings: `match`, zone `type` and `account.name` filters, and the
record filters `comment.*`, `content.*`, `name.*`, `tag.*`, `search`, `tag_match` and shadow
metadata; type-specific record fields (`priority`, `data`) are not modelled, and create rejects
types beyond the six listed above. Create comments, tags and advanced settings are excluded.
Timestamps are kept as
the RFC 3339 text Cloudflare sends.

## Tests

```sh
cargo test --locked -p concord-sdk-cloudflare-zones
```

`tests/workflows.rs` runs both workflows end to end (including `success: false` on a 200 in the
middle of a listing and on both detail calls); `tests/protocol.rs` checks the request shapes,
every error class above, the 429 policy and redaction. Fixtures are in `fixtures/`, each derived
from a cited example or labelled synthetic in [`PROVENANCE.md`](PROVENANCE.md). The Concord costs
met are in [`FRICTION.md`](FRICTION.md).
`tests/create_record.rs` covers validation, wire shape, duplicate refusal, single-attempt 429
and timeouts; `tests/create_consumer.rs` exercises public caller flows and post-timeout
reconciliation. Maintenance findings are in [`MAINTENANCE.md`](MAINTENANCE.md).
