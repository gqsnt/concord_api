# H02a design survey — a reusable value+metadata carrier (base `f921b002`)

Read-only survey by a Codex lane ({'model': 'gpt-6.1-sol', 'effort': 'xhigh', 'sandbox': 'read-only'} 2026-11-09T00:44:24Z 2026-11-09T00:55:56Z) for the coordinator; not a review or a decision. Its final message, verbatim:

**H02a has a real but modest maintenance case. I found no demonstrated current task failure that requires a new carrier.** The appropriate comparator is current `Received`/`decode_with_meta`, SDK aliases, and existing metadata helpers. A small optional carrier merits a narrow pilot; generated extraction has a substantially higher proof burden.

Surveyed commit: `f921b002b4844d79d610af1480ed5c182370e8da`, all 55 SDK directories. No edits, builds, or tests were run. Test references below describe checked-in assertions, not results obtained here. The supplied `v4/desirability/reviews/h01-design-survey.md` is absent from this checkout.

**1. Inventory**

Paths in the tables are relative to `examples/<SDK>-sdk/src/`. Classes follow A1 §3: **M** mechanical, **P** provider semantics, **S** safety/public boundary. No carrier was established as unexplained residue.

**LOC** counts the explicitly listed carrier/companion declarations and their own implementation blocks: nonblank, non-comment lines, including attributes. Free construction functions, metadata parsers, policies, tests, and imports are excluded. These are carrier-code counts, **not estimated deletions**.

`Pair` means `{ value: T, meta: M }`. Unless identified as provider-specific below, `M` is core `ResponseMeta`. All returned carriers have public callers as consumers; the table identifies additional page/stream consumers.

| SDK | Carrier fields | Construction and consumption evidence | LOC; class |
|---|---|---|---:|
| airtable | `Response<T>`: Pair | `lib.rs:80–85`; `decode_with_meta` at `:243–249`; page projection `:370–373` | 5; M |
| anthropic-messages | `Response<T>`: Pair; `MessageStream`: events, request ID, state, partial message, received count | `lib.rs:119–138,150–163`; stream consumer/state machine `stream.rs:182–287` | 94; M/P/S |
| digitalocean | `Response<T>`: Pair with provider metadata facade | `lib.rs:131–153`; pages retain metadata at `pagination.rs:159–162` | 11; M |
| dropbox | `Response<T>`: Pair; `FolderPageMeta`: HTTP metadata, cursor, `has_more` | `lib.rs:31–47`; construction `wire.rs:63–67`; pages `lib.rs:133–138` | 11; M/P |
| github | `WithMeta<T>`: Pair; `Conditional<T>`: modified value or metadata-only 304; `PageInfo`: response, optional total count | `meta.rs:179–216`; capture-before-decode `:277–292`; pagination consumes it at `paging.rs:614`, metadata definition `:324–329` | 33; M/P/S |
| gitlab | `Response<T>`: Pair with provider metadata | `meta.rs:16–30`; capture-before-decode/error retention `send.rs:42–61`; page construction `pages.rs:171` | 11; M/P |
| google-drive | `Response<T>`: Pair, also used for upload sessions | `error.rs:13–19`; bounded decode `client.rs:136–139`; validated session `upload.rs:129–155`; page projection `client.rs:190–193` | 5; M/S |
| intercom | `Response<T>`: Pair; `PageMeta`: total count, response metadata | `lib.rs:34–47,65–66`; `pages.rs:9–14,59–75` | 16; M/P |
| kraken-spot-rest | `Response<T>`: Pair after envelope validation; `TradesPageMeta`: offset, provider count, metadata | `policy.rs:15–28,152–162`; projection/validation `trades.rs:19–27,65–93` | 18; M/P/S |
| mailgun | `Response<T>`: Pair with `Metadata` facade | `policy.rs:66–79,111–115`; event pages consume it at `events.rs:292–306` | 11; M/P/S |
| notion | `Response<T>`: Pair | `client.rs:10–22,221–225`; page projection `:197`; incomplete-query error retains the response, `error.rs:122–125` | 11; M/S |
| openai | `Response<T>`: Pair; `EventStream`: SSE reader, metadata | `lib.rs:233–236,310–311,395–396`; stream consumer `:474–519` | 48; M/P/S |
| pokeapi | `Response<T>`: Pair with custom status, headers **and final URL** | `lib.rs:92–108`; explicit capture before decode `:274–287`; URL-preservation caller test `tests/workflows.rs:43` is described in `MAINTENANCE.md:43` | 5; M/P |
| snowflake | `Reply<T>`: Pair; `Partition`: index, rows, metadata, private partition plan | `client.rs:60–65,226–229`; partition construction/accessors `partitions.rs:182,203–225` | 24; M/P/S |
| stripe | `Response<T>`: Pair with selected Stripe metadata | `send.rs:137–142,473–477`; cursor pages use that metadata, `pages.rs:255` | 5; M/P |
| twilio | `Response<T>`: Pair | `error.rs:20–37,202–203`; public result documented at `lib.rs:61–65` | 13; M/S |

There are **16 exact generic Pair declarations** above. Their declarations and own implementations total **140 SLOC**, before additional page/stream types. That includes some provider accessors and formatting code, so even 140 is not a net deletion forecast.

The other direct response carriers are:

| SDK | Carrier fields | Construction → consumers | LOC; class |
|---|---|---|---:|
| adyen-checkout | `Response<T>`: value, metadata, retry report | `policy.rs:18–35`; successful, request-confirmed construction `:439–460` → payment callers | 14; M/P/S |
| amazon-s3 | `GetObjectOutput`: object head and body; `ObjectBody`: response, operation, expected/received lengths, metadata, failure; `PutObjectOutput`: ETag, version, expiration, encryption, size, metadata; `ListObjectsV2Output`: named listing fields, objects, prefixes, tokens, metadata; `ListPage`: prefixes, key count, response | `object.rs:19–131,203–221,247–383`; head/context extraction `:136–174,389–426`; XML carrier `xml.rs:71–98`; page projection `client.rs:472–479,535–542` → object readers/list traversal | 228; P/S, small M component |
| backblaze-b2-native | `Download`: bytes, verified file information, metadata; `FileNamePage`: files, next filename, metadata; `DeletedFileVersion`: name, ID, metadata | `download.rs:19–55,97–146`; `storage.rs:56–63,123,210–227`; underlying policy already returns `(T, ResponseMeta)`, `wire.rs:83–90` | 26; P/S/M |
| clickhouse | `Rows<T>`: reader, operation, format, marker state, metadata; `Collected<T>`: rows, completion outcome, metadata; `InsertReceipt`: requested query ID, body bytes, metadata | `rows.rs:55–189`; construction `client.rs:452–492`, `insert.rs:146–179`; callers consume rows and completion evidence | 125; P/S |
| cloudflare-zones | `Response<R>`: **result**, messages, errors, metadata; `PageResponse<R>` additionally has result information; `PageMeta`: page, total pages, result information, messages, errors, metadata | `policy.rs:19–45,173–198,215–245`; page projection `pagination.rs:34–47,129–140` | 27; P, embedded M |
| distribution-registry | `BlobInfo`: size/digest/meta; `BlobReader`: incoming body, hasher, expected digest, size/read counters, meta; `Manifest`: media type, digest, bytes, decoded image, meta; `ManifestPushed`: digest/meta; `UploadProgress`: offset/meta; `BlobPushed`: digest/size/meta | `blob.rs:29–36,148–285`; `manifest.rs:205–226,315,380`; `upload.rs:60–77,254,305,360`; catalog/tag pages already use core `Page<…,ResponseMeta>`, `pages.rs:83–89,152–159` | 136; P/S/M |
| frankfurter | `RateResponse`: **rate**, metadata | `lib.rs:58–63,88–89` → rate callers | 5; M, named public field |
| linear | `Response<T>`: **envelope**, metadata; `PageMeta`: HTTP metadata, GraphQL errors, extensions | `lib.rs:155–166,240–241,338–355,374–381` → callers deciding whether partial data is usable | 16; P/S, embedded M |
| mediawiki-action-api | `QueryResponse<T>`: **data**, continuation, metadata | `models.rs:118–125`; envelope validation/construction `policy.rs:21–39` → query/pagination callers | 6; P/M |
| microsoft-graph | Existing core `Page`: items, next link, `PageMeta { response, delta_link }` | `paging.rs:16–26,131–178` → bounded listing/delta callers | 5; P/S |
| nager-holidays | `HolidayResponse`: **holidays**, metadata | `lib.rs:43–48,65–69` → ordinary generated `send()` callers, documented at `:31–34` | 5; M, named public field |
| nasa-neows | `Metered<T>`: value, typed rate-limit observation | `lib.rs:143–148,228–232`; opt-in `send_metered`, while ordinary `send()` extracts only value, `:188–214` | 5; P/S |
| open-meteo | `ForecastResponse`: metadata, **data: ForecastData** | `lib.rs:96–137`; captures metadata, checks selected media, bounds bytes, decodes/validates JSON or CSV → forecast callers | 5; P/M |
| salesforce | Tuple `(T, Option<ApiUsage>)`; query `Page` metadata: total size, done, usage; result-set `Page` metadata: declared record count, bytes | `transport.rs:19–57`; `query.rs:157–167`; `bulk_query.rs:679–687` → query/result-set consumers | 11; P/M; tuple has no declaration |
| sentry | `Response<T>`: value, metadata, next cursor, frequency/concurrency limits, retry report | `model.rs:40–62`; header interpretation and construction `policy.rs:173–185,275–292`; page projection `pagination.rs:24–35` | 17; P/S/M |
| slack | `Response<T>`: value, metadata, warning, body response metadata; `UsersPageMeta`: HTTP metadata, warning, response metadata, cache timestamp | `lib.rs:31–40,86–95`; envelope handling `:251–279`; page projection `:189–199` | 14; P, embedded M |
| strava | `Response<T>`: value, metadata, quota observations, attempts; `PageMeta`: metadata, quotas, attempts | `lib.rs:53–71,162–172,202–209,247–260` → ordinary callers/activity pages | 21; P/S/M |
| usgs-earthquake-catalog | `Response<T>`: **data**, metadata; data may be an unread CSV reader | `policy.rs:13–26,85–86` → GeoJSON callers/streaming CSV consumers | 11; M/S |
| world-bank | `IndicatorPage`: provider page metadata, **observations**, response metadata; `TraversalMeta`: page metadata, response | `lib.rs:108–115,226–257,265–270,334–340` → indicator traversal | 11; P/S/M |
| zendesk-support | `CreatedTicket`: ticket, typed idempotency lookup; `TicketPage`: tickets, cursor metadata, links; lookup distinguishes hit/miss/absent/unrecognized | `models.rs:152–207`; **already extracted through `Decode::Context`**, `codecs.rs:16–46`; declared create response `lib.rs:89–91` | 21; P/S |

Important fields behind those metadata types must remain discoverable and typed:

- **GitHub:** request ID, rate limit/remaining/used/reset/resource, retry delay, ETag, Last-Modified, parsed links/errors, Location; `meta.rs:49–176`. **GitLab:** request ID, six pagination counters, next-page announcement, retry delay, parsed links/errors; `meta.rs:39–150`.
- **Stripe:** request ID, replay flag, retry advice, rate-limit reason, served API version; a 429 without the reason header is explicitly not classified as a rate limit, `send.rs:82–132`.
- **DigitalOcean/Mailgun:** reset semantics differ—oldest counted request expiry in seconds versus Unix milliseconds; `digitalocean-sdk/src/lib.rs:168–182`, `mailgun-sdk/src/policy.rs:29–60`.
- **Sentry/Strava/NASA:** concurrency versus frequency, overall versus non-upload quotas, and absent/invalid observations are different contracts; `sentry-sdk/src/rate.rs:6–38`, `strava-sdk/src/rate.rs:6–46`, `nasa-neows-sdk/src/lib.rs:115–138`.
- **ClickHouse:** query ID, format, timezone, server name, exception information, summary/progress counters; `meta.rs:61–118`. **Registry:** digest, upload UUID, API version and strict length/digest validation; `meta.rs:44–87`.
- **S3/Backblaze:** object identity, versions, checksums, ranges, lengths, user metadata and timestamps belong to the storage contract; `amazon-s3-sdk/src/object.rs:19–59`, `backblaze-b2-native-sdk/src/download.rs:19–34`.
- **Cloudflare/Slack:** named rate-limit items and body diagnostics/cursors cannot become an undifferentiated header bag; `cloudflare-zones-sdk/src/error.rs:37–49`, `slack-sdk/src/models.rs:35–48`.

For completeness, the remaining **19 SDKs add no separate successful HTTP-head carrier**. Their relevant provider-body envelopes are still P, not removable Pair boilerplate:

| SDK | Existing provider fields/carrier; construction/consumer evidence | Listed body-carrier LOC |
|---|---|---:|
| arxiv | `Feed`: entries, total/start/page size, problem; `model.rs:3–14`; decode/validate `policy.rs:76–89`, pages `lib.rs:141` | 8 |
| datadog-events | `EventsPage`: data, links, metadata containing request ID/status/warnings; projected `PageInfo`; `model.rs:17–54,126–151`, `client.rs:32–39,176–194` | 33 |
| elasticsearch | `SearchResponse`: PIT ID, elapsed time, timeout, shard outcome, hits; `pit.rs:336–349`; page projection `:442–471` | 12 |
| europe-pmc | Search/citation/reference pages retain hit counts and respective result lists; `model.rs:58–83,150–195`; consumers `pagination.rs:78,130` | 39 |
| fred | Search/observation envelopes retain count, offset, limit and records; `models.rs:42–51,75–84`; adapters `pagination.rs:14–23,52` | 24 |
| hubspot-crm | `SearchResponse`: total, results, paging; `lib.rs:113–132`; projection `:598–612` | 6 |
| kubernetes | `ObjectList`: kind/version, items, list metadata; `ListPageMeta`: snapshot/resource version, remaining count; `model/meta.rs:97–117`, `list.rs:157–165,302–325` | 22 |
| open-library | Work/author searches: documents, counts, start, count-exactness; author works: entries/size; `models.rs:96–107,133–144,206–211`; declared consumer `lib.rs:39` | 26 |
| postmark | `MessagePage`: total count/messages; `lib.rs:76–81,232,305` | 7 |
| quickbooks-online | Customer/query/invoice/payment response values retain response `time`; `models.rs:307–337,400–406`, `payment.rs:51–57`; page consumer `pages.rs:58` | 24 |
| rest-countries | `CountryPage`: countries, pagination/request metadata, demo information; conversion/validation `model.rs:23–129`; consumer `lib.rs:180` | 53 |
| shopify | `GraphqlResponse`: data state, errors, extensions/cost; page metadata retains errors/cost; `graphql.rs:245–284`, `connection.rs:89–94,279–282` | 37 |
| spotify-web-api | `Paging<T>`: items, href, limit, offset, previous/next, total; `models.rs:119–155`; projection `pages.rs:85` | 30 |
| square-payments | `PaymentsPage`: payments/cursor; `models.rs:188–203`; consumer `lib.rs:151` | 14 |
| xero-accounting | `InvoicePage`: invoices, next cursor, pagination; `lib.rs:325–332`; traversal `:282–295` | 6 |
| discord-rest, mastodon, ntfy, paypal | No additional successful value-plus-response-metadata carrier found. Error metadata and ordinary entity timestamps are outside this success-carrier count. | 0 each |

**2. What core already offers**

There is **no core `Response<T>`** at this commit. Core exports `Received`, `Incoming`, `Operation` and `ResponseMeta`; `Response<T>` is an SDK convention. See [core exports](/home/canarit/projects/Concord_Desirability_Loop/work/wt/h02s/crates/concord-core/src/lib.rs:39).

Already available:

- `receive()` exposes any status before consuming the body; `Received` offers borrowed head/status/version, final URL, operation, owned metadata and native escapes: `response.rs:165–203,353–364`.
- `decode_with_meta()` returns `(T, ResponseMeta)`; finite responses move headers, streams copy the head while retaining their body: `response.rs:313–338`.
- Ordinary successful decode returns only `T`, deliberately discarding the companion head: `prepared.rs:644–651`.
- Failures **after a head arrives already retain it**, including decode, limit and mid-body transport failures: `prepared.rs:665–715`; [error accessors](/home/canarit/projects/Concord_Desirability_Loop/work/wt/h02s/crates/concord-core/src/error.rs:237). `Error::status()` remains Status-only; `meta()` is narrower than `head_meta()`.
- `document()` preserves authoritative metadata beside any-status document decoding; this is existing machinery, not a missing H02a feature: `response.rs:253–310`.
- Strict singleton/unsigned-header mechanics already exist in `field.rs:27–68`; provider names, units and meanings remain SDK-owned.
- `Decode::Context` already supports typed extraction, as Zendesk and S3 demonstrate. Pagination already provides `Page<T,K,M>`: `concord-pagination/src/types.rs:7–25`.

Awkwardness remains: repeated Pair definitions and formatting, repeated provider accessors, manual final-URL retention, and the choice between value-only and metadata-preserving exits. A carrier alone does not repair later projection: arXiv obtains metadata but returns a metadata-free pagination error at `policy.rs:76–87`. That is an observed information loss, **not evidence that recovery is wrong**.

Documentation is not absent: [AUTHOR_GUIDE §5.11](/home/canarit/projects/Concord_Desirability_Loop/work/wt/h02s/docs/AUTHOR_GUIDE.md:1568) already explains these distinctions thoroughly. Caller-facing discoverability is a more credible documentation target than another author recipe.

**3. Options and expected breakages**

| Option | What could disappear; what must remain | Concept cost and expected breakages for a card |
|---|---|---|
| **(a) Keep current; improve navigation/examples/aliases** | No justified production deletion established. Could prevent obsolete native-execution recipes from being copied. Preserve provider names, errors and streaming unchanged. | Lowest cost. Expected API/compiler breakages: **none**. Documentation/example changes only; independently verify every signature shown. |
| **(b) Optional `WithMeta<T>`**, possibly `WithMeta<T,M=ResponseMeta>` | Could replace M Pair shells and some repeated safe formatting. Typed metadata facades, provider envelopes, validation, error classification, retry reports and stream state remain. Keep named `holidays`, `rate`, `result`, `observations`, etc. where meaningful. | One extra type, ownership/accessor rules, formatting contract; optional `M` adds another parameter. Non-adopters should have **zero breaks**. Adopters may change nominal types, aliases, implementation locations, trait bounds, imports and documented result types. Moving SDK inherent methods onto an alias of a foreign core type is an **expected compiler constraint**, requiring an explicit replacement design and compile witness. |
| **(c) DSL-declared extraction** | Could generate selected field capture/accessors and carrier assembly. Provider meaning moves into declarations rather than disappearing. Existing `Decode::Context` is the comparator. | Highest cost: syntax, extraction order, repeated/missing/invalid-field rules, units, failure ownership and stream interaction. Expected breaks must enumerate migrated DSL sites, generated signatures and changed caller matches. No unregistered changes to singleton handling, 304/202 handling, strict media checks, limits or stream laziness. |

For both candidates, kill conditions should include lost named fields, weaker metadata on failures, changed quota/retry interpretation, newly printed values, new `Clone`/`Debug` requirements, eager stream consumption, and required ceremony in controls. Those are concrete preservation obligations, not acceptable side effects of fewer types. A3 expressly requires expected break sites/classes before implementation: `PLAN_AMENDMENT_A3.md:66–72`.

**4. Deficit and a checkable pilot**

**Observed records support duplication; they do not establish a current recovery defect.**

- GitHub F1 records about 70 historical plumbing lines, but explicitly marks the execution problem addressed by `Received`: `github-sdk/FRICTION.md:15–53`. Current metadata interpretation is **98 SLOC** over `meta.rs:31–176`; GitLab’s corresponding range is **81 SLOC**, `meta.rs:32–150`. Most of that is P, not a generic carrier saving.
- Airtable calls its adapter repeated library-level work, but reports metadata preservation, not loss: `airtable-sdk/FRICTION.md:6`. Anthropic identifies remaining generic scaffolding while noting that adding an endpoint required no copied error logic: `anthropic-messages-sdk/MAINTENANCE.md:115–125`.
- Nager’s historical capture-before-decode complaint, `FRICTION.md:39–52`, is superseded by current core and its own `lib.rs:65–69`.
- Linear’s maintenance record claims null-connection diagnostics are lost, `MAINTENANCE.md:158–163`; **current source contradicts that stale note**. `IssuesData` permits a null connection, and `Error::Page` retains the response: `lib.rs:106–111,312–335`. The current [regression test](/home/canarit/projects/Concord_Desirability_Loop/work/wt/h02s/examples/linear-sdk/tests/first_use.rs:316) asserts retained diagnostics.
- Core’s [metadata-terminal tests](/home/canarit/projects/Concord_Desirability_Loop/work/wt/h02s/checks/core-concepts/tests/x9_metadata_terminal.rs:24) assert success metadata and retention through decode, limit, transport, drain and media/status failures. These are existing controls, not a deficit to manufacture.
- GitHub’s older item-flattening complaint, `FRICTION.md:161–177`, is also bounded by current metadata-preserving `PageItemEvents`, `concord-pagination/src/pages.rs:383–458`; H02a should not claim that capability as missing.

A defensible pilot would pre-register:

| Measure | Witnesses / automatic oracle |
|---|---|
| **Mechanical maintenance benefit** | Exact Pair inventory; changed carrier/alias/accessor sites; total added/deleted production code across core **and** SDKs. GitHub/GitLab expose the largest compared metadata facades; use **Twilio or Airtable** for simple Pair mechanics and **Anthropic** for an unlike streaming case. |
| **Complete ordinary use** | Compile existing calls and explicit result annotations; then independently check decoded value, status, request ID and ownership. Compiler outcomes remain **expectations** until executed. |
| **Preserved recovery facts** | Reuse the core failure matrix; Stripe’s reason-sensitive 429 classification (`tests/wire.rs:358–443`) and Sentry’s invalid/repeated quota tests (`tests/protocol.rs:408–451`). A smaller carrier must not alter these decisions. |
| **Contrary cases** | Cloudflare named diagnostics/result information; Linear partial data; S3 or ClickHouse streaming/completion evidence. Compare public fields and error variants before/after, then exercise existing fixtures. |
| **Controls** | **Nager:** retain `.holidays`; **Frankfurter:** retain `.rate`; **PokeAPI:** retain final `.meta.url` and URL validation. No required imports, wrappers or extraction declarations added. |
| **Maintenance task outcome** | Add one provider metadata fact; count authoritative edit sites, missed propagation and detection stage. Line reduction alone cannot establish a win under A1 §2. |

The present deficit is **weak at task level and credible at the level of small repeated mechanics**. Nothing inspected supports treating a universal response shape—or DSL extraction—as necessary. Human preference and adoption remain unmeasured.