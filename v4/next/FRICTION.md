# Friction catalogue (55 SDKs)

Sources and method: every line of `examples/*-sdk/{FRICTION,MAINTENANCE,FIRST_USE}.md` (55 / 50 / 50 files) and of `v4/sdk50/cross50/PROPOSALS.md` + `analysts/*.md` was read (11 parallel readers, one per 5 SDKs, plus one for cross50), giving 747 raw (SDK, friction) rows; the coordinator merged them by mechanism into the entries below, so one row can cover several wordings, and different mechanisms stay apart even when they share a seed idea.
Counts are distinct SDKs whose own notes report the friction (cross50 is used as a cross-check and for exclusions, never to add an SDK). Excluded: positive notes, provider quirks with no Concord angle, SDK-only README/code issues, consumer slips, toolchain issues, 30 rows the notes mark fixed by a later change (twilio's SDK-side workaround in A2 is kept), 9 rows cross50 shows to be historical (named in the affected rows) and 3 rows judged not to be about Concord. Docs-only fixes are kept and marked.
Evidence paths are relative to the repository root. Rows where only git history (not a note) hints at a later fix were kept as reported; families O and P are new (no seed family fitted); N has no entry because no SDK note reports a removal candidate.

Totals: A 15; B 10; C 11; D 6; E 9; F 18; G 7; H 23; I 9; J 1; K 10; L 7; M 7; N 0; O 32; P 25; 190 entries over 55 SDKs.

## A. Errors

| # | Friction (one line) | SDKs | Count | Evidence |
|---|---|---|---|---|
| A1 | Provider error document cannot be declared: SDK hand-writes the status branch, bounded error-body read, typed error document and its own error type (From/Display/accessors) around `concord::Error` | adyen-checkout, airtable, amazon-s3, anthropic-messages, arxiv, backblaze-b2-native, cloudflare-zones, datadog-events, digitalocean, dropbox, frankfurter, fred, google-drive, hubspot-crm, intercom, kraken-spot-rest, kubernetes, linear, mailgun, mastodon, mediawiki-action-api, microsoft-graph, nager-holidays, notion, open-library, open-meteo, pokeapi, postmark, rest-countries, salesforce, sentry, snowflake, spotify-web-api, square-payments, strava, stripe, twilio, world-bank | 38 | examples/mastodon-sdk/FRICTION.md:118; examples/rest-countries-sdk/FRICTION.md:64; examples/frankfurter-sdk/FRICTION.md:24 |
| A2 | Generated rustdoc prints bare `Error` for `concord::Error`, the SDK error and capability-crate errors alike; callers cannot tell which one they hold *(twilio worked around it by renaming its SDK error; Concord unchanged)* | adyen-checkout, arxiv, datadog-events, digitalocean, dropbox, europe-pmc, frankfurter, kraken-spot-rest, mastodon, rest-countries, sentry, spotify-web-api, square-payments, strava, twilio, xero-accounting | 16 | examples/mastodon-sdk/FRICTION.md:132; examples/spotify-web-api-sdk/FRICTION.md:123; examples/digitalocean-sdk/FIRST_USE.md:211 |
| A3 | Provider failure inside a 2xx body (`ok:false`, `success:false`, `errors` beside data, Atom error feed, trailing exception block) cannot be declared; SDK detects it by hand | arxiv, clickhouse, cloudflare-zones, elasticsearch, kraken-spot-rest, linear, mediawiki-action-api, quickbooks-online, slack, world-bank, xero-accounting | 11 | examples/slack-sdk/FRICTION.md:16; examples/kraken-spot-rest-sdk/FRICTION.md:49; examples/cloudflare-zones-sdk/FRICTION.md:28 |
| A4 | Adding an error variant or provider error code means parallel hand edits (enum, `Display`, `status()`, parser arm). Exhaustive matches catch an omitted arm (E0004), but a fallback "unknown code" arm hides a code the parser forgot | backblaze-b2-native, cloudflare-zones, digitalocean, fred, kraken-spot-rest | 5 | examples/backblaze-b2-native-sdk/MAINTENANCE.md:100; examples/cloudflare-zones-sdk/MAINTENANCE.md:99 |
| A5 | Decode errors name only line/column, not the missing field or column, so a model mismatch looks like a broken response | intercom, mediawiki-action-api, notion, usgs-earthquake-catalog | 4 | examples/mediawiki-action-api-sdk/FIRST_USE.md:207; examples/intercom-sdk/FIRST_USE.md:13; examples/notion-sdk/FIRST_USE.md:17 |
| A6 | Failure facts are hard to reach: stage/kind, unread body, operation and timeout need several layers or consume the error | europe-pmc, kubernetes, pokeapi, square-payments | 4 | examples/europe-pmc-sdk/FIRST_USE.md:226; examples/kubernetes-sdk/FRICTION.md:126; examples/pokeapi-sdk/FIRST_USE.md:139 |
| A7 | SDK re-creates an error-body stage enum (`Metadata/Read/Decode`) that `Error::kind()` already covers *(cross50: existing `kind()` already covers it, so this is adoption)* | datadog-events, open-meteo, rest-countries | 3 | examples/datadog-events-sdk/FRICTION.md:67; examples/rest-countries-sdk/FRICTION.md:66; examples/open-meteo-sdk/FRICTION.md:93 |
| A8 | A decode failure drops the body or the head, so provider diagnostics are lost; SDK clones them before decoding | linear, nager-holidays | 2 | examples/linear-sdk/FRICTION.md:7; examples/nager-holidays-sdk/FRICTION.md:39 |
| A9 | A typed failure raised inside a codec or byte source comes back only by unredacted downcast, not by value | clickhouse, quickbooks-online | 2 | examples/clickhouse-sdk/FRICTION.md:55; examples/quickbooks-online-sdk/FRICTION.md:81 |
| A10 | Error body uses another format than the success body (XML or JSON, JSON on a CSV endpoint); SDK selects the parser by hand | fred, open-meteo | 2 | examples/fred-sdk/FRICTION.md:16; examples/open-meteo-sdk/FRICTION.md:92 |
| A11 | Schema violations surface as `concord::Error` Decode while SDK checks surface as SDK errors; callers misclassify the same kind of fault | microsoft-graph, snowflake | 2 | examples/microsoft-graph-sdk/FIRST_USE.md:243; examples/snowflake-sdk/FIRST_USE.md:162 |
| A12 | Caller-facing decode error text gives SDK-author advice ("omit `->` ...") | sentry | 1 | examples/sentry-sdk/FIRST_USE.md:265 |
| A13 | SDKs cannot construct a `concord::Error`, so SDK-detected failures need extra variants | postmark | 1 | examples/postmark-sdk/MAINTENANCE.md:133 |
| A14 | Client setup returns three unrelated error types (reqwest, `CredentialError`, `concord::Error`), so setup cannot `?` into one | square-payments | 1 | examples/square-payments-sdk/FIRST_USE.md:203 |
| A15 | `CodecError` is large, so clippy `result_large_err` forces every wrapping SDK error to box it | salesforce | 1 | examples/salesforce-sdk/FRICTION.md:128 |

## B. Results, envelopes and metadata

| # | Friction (one line) | SDKs | Count | Evidence |
|---|---|---|---|---|
| B1 | No core value-plus-metadata result: SDK defines its own `Response<T>` and captures the response head by hand | airtable, anthropic-messages, cloudflare-zones, digitalocean, dropbox, frankfurter, intercom, kraken-spot-rest, linear, mailgun, mediawiki-action-api, nager-holidays, nasa-neows, pokeapi, slack, strava, twilio, world-bank | 18 | examples/kraken-spot-rest-sdk/FRICTION.md:42; examples/digitalocean-sdk/FRICTION.md:47; examples/anthropic-messages-sdk/MAINTENANCE.md:117 |
| B2 | Generated `send()` rustdoc shows a projection (`<P as SendWith<T>>::Output`, `Records<F, I>`, wire `Envelope<T>`) instead of the concrete result or reader type *(postmark and zendesk excluded: cross50 a3 says explicit result templates now fix theirs)* | anthropic-messages, cloudflare-zones, discord-rest, kraken-spot-rest, mastodon, mediawiki-action-api, nasa-neows, quickbooks-online, usgs-earthquake-catalog | 9 | examples/cloudflare-zones-sdk/FRICTION.md:13; examples/kraken-spot-rest-sdk/FRICTION.md:38; examples/mediawiki-action-api-sdk/FRICTION.md:34 |
| B3 | Success envelope projected by hand; the send-result template's `_` binds the whole wire envelope, not its inner value | cloudflare-zones, kraken-spot-rest, mediawiki-action-api, quickbooks-online, world-bank, zendesk-support | 6 | examples/mediawiki-action-api-sdk/FRICTION.md:13; examples/cloudflare-zones-sdk/FRICTION.md:12; examples/kraken-spot-rest-sdk/FRICTION.md:34 |
| B4 | Policy-preserving `send()`/`pages()` return only the value; success headers need `prepare().receive()`, which bypasses the send policy | paypal, postmark, square-payments, zendesk-support | 4 | examples/postmark-sdk/FIRST_USE.md:127; examples/square-payments-sdk/FIRST_USE.md:218; examples/zendesk-support-sdk/FRICTION.md:110 |
| B5 | Status-dependent outcomes cannot be declared (308 progress, 202 vs 200 types, 204/404 as no-data); SDK dispatches by status by hand | google-drive, snowflake, usgs-earthquake-catalog | 3 | examples/google-drive-sdk/FRICTION.md:41; examples/snowflake-sdk/FRICTION.md:23; examples/usgs-earthquake-catalog-sdk/FRICTION.md:14 |
| B6 | Under a send policy the declared output and body types must be public, contrary to the author guide's private-output advice | cloudflare-zones, elasticsearch | 2 | examples/cloudflare-zones-sdk/FRICTION.md:102; examples/elasticsearch-sdk/FRICTION.md:120 |
| B7 | One response has two shapes: `send()` returns the decoded document, `pages()` returns `Page{items,next,meta}` with other field names | europe-pmc, xero-accounting | 2 | examples/europe-pmc-sdk/FIRST_USE.md:235; examples/xero-accounting-sdk/FIRST_USE.md:179 |
| B8 | Multi-status batch results (207, per-key outcomes) need hand per-key bookkeeping; no guidance on partial results | hubspot-crm | 1 | examples/hubspot-crm-sdk/MAINTENANCE.md:127 |
| B9 | `SendWith::Output` is one type per client; adding metadata after release breaks every `send()` caller or needs a parallel terminal | nasa-neows | 1 | examples/nasa-neows-sdk/MAINTENANCE.md:44 |
| B10 | `Received::head` carries no URL, so relative `Link` targets need the native response | github | 1 | examples/github-sdk/FRICTION.md:51 |

## C. Construction: fixed vs changeable values, base URL, transport

| # | Friction (one line) | SDKs | Count | Evidence |
|---|---|---|---|---|
| C1 | Safe transport (redirects off, native retries off, timeouts) is built by hand in each SDK; Concord cannot check or enforce the supplied reqwest client | adyen-checkout, anthropic-messages, datadog-events, digitalocean, distribution-registry, elasticsearch, fred, github, gitlab, hubspot-crm, kraken-spot-rest, mailgun, microsoft-graph, postmark, spotify-web-api, square-payments, strava, twilio, usgs-earthquake-catalog | 19 | examples/datadog-events-sdk/FRICTION.md:111; examples/anthropic-messages-sdk/FRICTION.md:66; examples/gitlab-sdk/FRICTION.md:87 |
| C2 | Base vs path boundary of `with_base_url` is unclear: callers duplicate or lose the declared prefix (`/v0/v0`, `/client/v4/client/v4`) | airtable, cloudflare-zones, nasa-neows, openai, sentry, spotify-web-api, strava, zendesk-support | 8 | examples/cloudflare-zones-sdk/FIRST_USE.md:130; examples/openai-sdk/FRICTION.md:10; examples/zendesk-support-sdk/FIRST_USE.md:128 |
| C3 | Generated public constructor, setters and `with_base_url` bypass SDK invariants (credential validation, token origin, tenant, validated inputs); SDK hides or wraps the generated client | adyen-checkout, digitalocean, gitlab, mastodon, ntfy, paypal, usgs-earthquake-catalog, xero-accounting | 8 | examples/mastodon-sdk/FRICTION.md:33; examples/xero-accounting-sdk/FRICTION.md:15; examples/paypal-sdk/FRICTION.md:95 |
| C4 | A server-issued target URL cannot be addressed directly: SDK prepares a placeholder declared endpoint and swaps the URL, or builds a raw request *(pokeapi and salesforce excluded: cross50 a1/a4 call theirs historical since `Prepared::follow` exists)* | backblaze-b2-native, distribution-registry, github, gitlab, shopify | 5 | examples/backblaze-b2-native-sdk/FRICTION.md:30; examples/distribution-registry-sdk/FRICTION.md:121; examples/github-sdk/FRICTION.md:68 |
| C5 | A base known only at run time (self-hosted instance, site-dependent parent domain, issued per-role hosts, second advertised origin) cannot be declared; SDK declares a placeholder and rebinds | backblaze-b2-native, datadog-events, elasticsearch, mastodon | 4 | examples/mastodon-sdk/FRICTION.md:40; examples/backblaze-b2-native-sdk/FRICTION.md:10; examples/datadog-events-sdk/FRICTION.md:34 |
| C6 | `Prepared::follow` checks only the origin; binding a returned link to account, path and link kind is hand-written | microsoft-graph, snowflake | 2 | examples/microsoft-graph-sdk/FRICTION.md:33; examples/snowflake-sdk/MAINTENANCE.md:23 |
| C7 | `with_base_url` refuses a query, so an issued URL whose query matters (token realm, signed download) cannot be a base | distribution-registry, shopify | 2 | examples/distribution-registry-sdk/FRICTION.md:147; examples/shopify-sdk/FRICTION.md:86 |
| C8 | Values that belong together cannot be tied: a realm and its token, or a policy's site and the base, can drift apart | datadog-events, quickbooks-online | 2 | examples/quickbooks-online-sdk/FRICTION.md:19; examples/datadog-events-sdk/FRICTION.md:101 |
| C9 | Two client declarations on the same host cannot share one base; the base literal is restated | paypal | 1 | examples/paypal-sdk/FRICTION.md:10 |
| C10 | An OAuth token endpoint has to be a second `api!` client rebuilt per token request to keep its base configurable | quickbooks-online | 1 | examples/quickbooks-online-sdk/FRICTION.md:115 |
| C11 | Generated `new()` is infallible; header placement checks wait until the first `prepare()` | datadog-events | 1 | examples/datadog-events-sdk/FRICTION.md:78 |

## D. Secrets in SDK types and redaction

| # | Friction (one line) | SDKs | Count | Evidence |
|---|---|---|---|---|
| D1 | SDK-owned models, errors, cursors and inputs need hand-written redacting `Debug`/`Display`; a derived `Debug` compiles and leaks | adyen-checkout, datadog-events, dropbox, elasticsearch, frankfurter, fred, hubspot-crm, intercom, kraken-spot-rest, kubernetes, mediawiki-action-api, nager-holidays, open-library, rest-countries, sentry, square-payments, strava, usgs-earthquake-catalog | 18 | examples/elasticsearch-sdk/FRICTION.md:105; examples/adyen-checkout-sdk/FRICTION.md:52; examples/intercom-sdk/FRICTION.md:15 |
| D2 | `#[sensitive]` covers only declared placed values: runtime `query(..)` pairs, natively appended pairs and bodies are outside it | clickhouse, hubspot-crm, mediawiki-action-api | 3 | examples/mediawiki-action-api-sdk/FRICTION.md:47; examples/hubspot-crm-sdk/FRICTION.md:43; examples/clickhouse-sdk/FRICTION.md:26 |
| D3 | A secret placed as wire text (a header template, or a query value such as Kubernetes' continue token) must implement `Display`, which prints the raw secret | kubernetes, ntfy, stripe | 3 | examples/kubernetes-sdk/FRICTION.md:192; examples/ntfy-sdk/FRICTION.md:115; examples/stripe-sdk/FRICTION.md:156 |
| D4 | Over-redaction hides non-secret values (policy value, sibling literal headers, a whole followed URL) and loses diagnostics | distribution-registry, quickbooks-online, zendesk-support | 3 | examples/quickbooks-online-sdk/FRICTION.md:24; examples/zendesk-support-sdk/FRICTION.md:25; examples/distribution-registry-sdk/FRICTION.md:92 |
| D5 | Native or unredacted accessors leak (`concord::Error` native response, raw `reqwest::Error` printing a signed URL); SDK projects errors to stay safe | nasa-neows, shopify | 2 | examples/nasa-neows-sdk/FRICTION.md:6; examples/shopify-sdk/FRICTION.md:91 |
| D6 | `#[sensitive]` must be repeated on every copy of a parameter, and the name-based safety net misses `continue` | kubernetes | 1 | examples/kubernetes-sdk/FRICTION.md:30 |

## E. Operation facts

| # | Friction (one line) | SDKs | Count | Evidence |
|---|---|---|---|---|
| E1 | Declared values are not readable: the send policy sees only the built request and handles have no getters, so SDKs re-parse values from the prepared URL, headers or body by name | adyen-checkout, amazon-s3, arxiv, cloudflare-zones, discord-rest, europe-pmc, gitlab, microsoft-graph, paypal, pokeapi, postmark, quickbooks-online, stripe, usgs-earthquake-catalog, xero-accounting, zendesk-support | 16 | examples/cloudflare-zones-sdk/MAINTENANCE.md:116; examples/discord-rest-sdk/FRICTION.md:45; examples/zendesk-support-sdk/FRICTION.md:64 |
| E2 | "Not sent" vs "maybe applied" for a write is classified by hand from `Delivery`, reqwest's error taxonomy and the method | backblaze-b2-native, cloudflare-zones, discord-rest, elasticsearch, google-drive, hubspot-crm, sentry, spotify-web-api, square-payments, strava, zendesk-support | 11 | examples/zendesk-support-sdk/FRICTION.md:124; examples/discord-rest-sdk/FRICTION.md:100; examples/sentry-sdk/MAINTENANCE.md:146 |
| E3 | Policies classify operations by HTTP method or a hand route list; a new endpoint silently joins the wrong rule (compiles, only tests catch it) | cloudflare-zones, kraken-spot-rest, paypal, postmark, sentry, spotify-web-api, square-payments, strava, zendesk-support | 9 | examples/strava-sdk/MAINTENANCE.md:94; examples/square-payments-sdk/MAINTENANCE.md:85; examples/kraken-spot-rest-sdk/MAINTENANCE.md:98 |
| E4 | Send policy is client-only; scope- or endpoint-specific handling needs a second client, output-type dispatch or identity checks | backblaze-b2-native, datadog-events, elasticsearch, paypal, quickbooks-online | 5 | examples/quickbooks-online-sdk/FRICTION.md:9; examples/datadog-events-sdk/MAINTENANCE.md:126; examples/backblaze-b2-native-sdk/FRICTION.md:48 |
| E5 | Operation identity is awkward to use: string-name comparisons, two representations (`Operation` vs `&'static str`), not `Hash` *(square excluded: cross50 a2 says its `<E as Endpoint<T>>::operation()` complaint is historical)* | discord-rest, ntfy, strava, zendesk-support | 4 | examples/strava-sdk/MAINTENANCE.md:187; examples/zendesk-support-sdk/MAINTENANCE.md:53; examples/ntfy-sdk/FRICTION.md:133 |
| E6 | `operation()` accessors return `Option` even for generated endpoints after a successful send | amazon-s3, frankfurter | 2 | examples/amazon-s3-sdk/FRICTION.md:156; examples/frankfurter-sdk/FIRST_USE.md:185 |
| E7 | A required per-write request/idempotency ID cannot be declared once; the default is repeated per endpoint and the ID generator is hand-written | quickbooks-online | 1 | examples/quickbooks-online-sdk/MAINTENANCE.md:78 |
| E8 | No shared replay/idempotency ledger; exact retry identity, in-flight guards and retention are hand-written | adyen-checkout | 1 | examples/adyen-checkout-sdk/FRICTION.md:23 |
| E9 | A non-idempotent write cannot be declared non-replayable for transport retries; SDK wraps the body by hand | datadog-events | 1 | examples/datadog-events-sdk/MAINTENANCE.md:74 |

## F. Paging facts

| # | Friction (one line) | SDKs | Count | Evidence |
|---|---|---|---|---|
| F1 | Paging cannot be declared: each paged operation needs a hand adapter (clone context into a page factory, rebuild the endpoint, project cursor or next link from body/header into `Page`) *(cross50: b2's hand loop could use the existing sequential engine)* | airtable, arxiv, backblaze-b2-native, digitalocean, dropbox, europe-pmc, hubspot-crm, intercom, kraken-spot-rest, linear, mailgun, notion, sentry, shopify, slack, square-payments, world-bank | 17 | examples/digitalocean-sdk/FRICTION.md:62; examples/intercom-sdk/FRICTION.md:14; examples/airtable-sdk/FRICTION.md:7 |
| F2 | concord-pagination vocabulary is hard to learn from SDK docs (extra `Resume` layer, `Incomplete.collected` vs `items`, `Items` vs `PageItems`, `None` after an error, missing aliases) *(docs-only fix: arxiv)* | airtable, arxiv, digitalocean, discord-rest, elasticsearch, google-drive, hubspot-crm, mailgun, rest-countries, slack, strava, xero-accounting | 12 | examples/xero-accounting-sdk/FRICTION.md:65; examples/google-drive-sdk/FIRST_USE.md:15; examples/airtable-sdk/FIRST_USE.md:167 |
| F3 | Pagination does not refuse a repeated or non-progressing cursor; SDK keeps its own seen-set or relies on caps | datadog-events, europe-pmc, hubspot-crm, intercom, linear, mailgun, notion, sentry, slack, zendesk-support | 10 | examples/datadog-events-sdk/FRICTION.md:51; examples/zendesk-support-sdk/FRICTION.md:89; examples/mailgun-sdk/FRICTION.md:14 |
| F4 | A resume key is not bound to the listing that produced it (filters, page size, account); SDK carries and compares context by hand | airtable, backblaze-b2-native, digitalocean, europe-pmc, intercom, mailgun, stripe, world-bank, xero-accounting | 9 | examples/xero-accounting-sdk/FRICTION.md:47; examples/mailgun-sdk/FRICTION.md:14; examples/intercom-sdk/FRICTION.md:14 |
| F5 | `items()` / `collect_complete()` drop page metadata (request ids, quota headers, terminal tokens); SDK walks pages instead | digitalocean, dropbox, github, google-drive, linear, microsoft-graph, slack, stripe | 8 | examples/github-sdk/FRICTION.md:161; examples/microsoft-graph-sdk/FRICTION.md:48; examples/slack-sdk/FIRST_USE.md:24 |
| F6 | `PagesError`/`Incomplete` `Display`/`Debug` hide the underlying cause | digitalocean, dropbox, microsoft-graph, xero-accounting, zendesk-support | 5 | examples/digitalocean-sdk/FIRST_USE.md:251; examples/dropbox-sdk/FIRST_USE.md:188; examples/zendesk-support-sdk/FIRST_USE.md:133 |
| F7 | A traversal cannot start from a given position (resume key, item checkpoint, already-fetched first page); SDKs add `_from` twins or drop to the engine | digitalocean, github, notion, snowflake, spotify-web-api | 5 | examples/digitalocean-sdk/MAINTENANCE.md:136; examples/spotify-web-api-sdk/FRICTION.md:106; examples/notion-sdk/FRICTION.md:50 |
| F8 | `PagesError::Provider` also carries SDK-side or local refusals (repeated cursor, untrusted link, provider limit), so its name misleads *(docs-only fix: europe-pmc)* | arxiv, europe-pmc, mailgun, sentry | 4 | examples/mailgun-sdk/FIRST_USE.md:246; examples/sentry-sdk/FIRST_USE.md:272; examples/arxiv-sdk/FIRST_USE.md:207 |
| F9 | RFC 8288 `Link` headers parsed by hand (shared parser exposes only raw parameter values) | distribution-registry, gitlab, sentry, snowflake | 4 | examples/gitlab-sdk/FRICTION.md:62; examples/distribution-registry-sdk/FRICTION.md:161; examples/sentry-sdk/FRICTION.md:13 |
| F10 | No continuation helper beyond `next_offset`: page-number/page-count and offset-walk traversals are hand-written | europe-pmc, kraken-spot-rest, world-bank, xero-accounting | 4 | examples/xero-accounting-sdk/FRICTION.md:45; examples/europe-pmc-sdk/MAINTENANCE.md:79; examples/kraken-spot-rest-sdk/MAINTENANCE.md:139 |
| F11 | Traversal failures need two-layer matching (`PagesError`, then the SDK error) with no direct accessor | gitlab, microsoft-graph, world-bank | 3 | examples/microsoft-graph-sdk/FIRST_USE.md:231; examples/gitlab-sdk/FIRST_USE.md:136; examples/world-bank-sdk/FIRST_USE.md:151 |
| F12 | Traversal types are hard to name or too large (boxed closure aliases, clippy `type_complexity` / `result_large_err`) | microsoft-graph, snowflake, strava | 3 | examples/snowflake-sdk/FRICTION.md:46; examples/microsoft-graph-sdk/FIRST_USE.md:224; examples/strava-sdk/FRICTION.md:15 |
| F13 | A page whose continuation is refused fails whole: its valid records are dropped and resume repeats the failure | digitalocean, mailgun | 2 | examples/digitalocean-sdk/FIRST_USE.md:242; examples/mailgun-sdk/FIRST_USE.md:252 |
| F14 | Lower-level engine reports (`SequentialReport`, independent / manifest jobs) are awkward to map to SDK outcomes | kubernetes, salesforce | 2 | examples/salesforce-sdk/FRICTION.md:154; examples/kubernetes-sdk/FRICTION.md:172 |
| F15 | Pages cannot drive merge-until-answered property continuation over a fixed key set | mediawiki-action-api | 1 | examples/mediawiki-action-api-sdk/MAINTENANCE.md:99 |
| F16 | Pages keeps only the page key; extra per-page state (point-in-time id) needs `Arc<Mutex>` side channels | elasticsearch | 1 | examples/elasticsearch-sdk/FRICTION.md:75 |
| F17 | No paging adapter for incremental (streamed) readers; cross-page streaming traversal is hand-written | usgs-earthquake-catalog | 1 | examples/usgs-earthquake-catalog-sdk/FRICTION.md:13 |
| F18 | `Items::checkpoint()` after `next()` already counts the item; no checkpoint that re-emits it | github | 1 | examples/github-sdk/FRICTION.md:179 |

## G. Facades, naming, forwarders and re-exports

| # | Friction (one line) | SDKs | Count | Evidence |
|---|---|---|---|---|
| G1 | Concord types callers need (`ErrorKind`, pagination, retry/ratelimit/auth payloads, `Bytes`) are not re-exported where callers look; apps add version-matched direct dependencies | adyen-checkout, arxiv, frankfurter, kraken-spot-rest, mailgun, mediawiki-action-api, microsoft-graph, sentry, snowflake, square-payments, strava, xero-accounting, zendesk-support | 13 | examples/strava-sdk/FIRST_USE.md:233; examples/microsoft-graph-sdk/FIRST_USE.md:247; examples/square-payments-sdk/FIRST_USE.md:199 |
| G2 | Hand-written forwarders / domain methods over generated endpoints (to return SDK results, hide the client, add paging or typed lookups) | datadog-events, digitalocean, distribution-registry, elasticsearch, google-drive, hubspot-crm, intercom, mastodon, rest-countries, slack, twilio | 11 | examples/mastodon-sdk/FRICTION.md:38; examples/twilio-sdk/FRICTION.md:21; examples/rest-countries-sdk/MAINTENANCE.md:116 |
| G3 | Generated request/endpoint types share no trait; generic SDK code needs a hand trait implemented by macro or a dyn factory | kubernetes, microsoft-graph, nasa-neows, shopify, stripe | 5 | examples/kubernetes-sdk/FRICTION.md:36; examples/stripe-sdk/FRICTION.md:109; examples/microsoft-graph-sdk/FRICTION.md:84 |
| G4 | Operation names come from declaration identifiers (`Client.scope.method`, private client names), not public types or provider names; callers mispredict them *(docs-only fix: twilio)* | backblaze-b2-native, distribution-registry, snowflake, twilio | 4 | examples/distribution-registry-sdk/FRICTION.md:134; examples/backblaze-b2-native-sdk/FIRST_USE.md:126; examples/snowflake-sdk/MAINTENANCE.md:102 |
| G5 | Generated `set_`/`maybe_` setter names surprise (`request_id` vs `set_request_id`, `Option` passed to `set_`); SDKs add aliases | backblaze-b2-native, paypal, stripe | 3 | examples/backblaze-b2-native-sdk/MAINTENANCE.md:18; examples/paypal-sdk/MAINTENANCE.md:84; examples/stripe-sdk/FRICTION.md:77 |
| G6 | Generated type names need `pub use` aliases, stutter (`OAuthOauth`) or can collide (scope + endpoint concatenation) | adyen-checkout, mastodon, strava | 3 | examples/mastodon-sdk/FRICTION.md:38; examples/strava-sdk/FIRST_USE.md:222; examples/adyen-checkout-sdk/MAINTENANCE.md:148 |
| G7 | `#[must_use]` on a wrapper returning a generated `#[must_use]` type trips clippy `double_must_use` | discord-rest | 1 | examples/discord-rest-sdk/FRICTION.md:130 |

## H. Generated documentation and guides

| # | Friction (one line) | SDKs | Count | Evidence |
|---|---|---|---|---|
| H1 | SDK rustdoc built with `--no-deps` leaves Concord types unlinked; consumers build Concord crates' docs separately to learn errors, metadata and pagination | adyen-checkout, arxiv, backblaze-b2-native, cloudflare-zones, datadog-events, digitalocean, elasticsearch, fred, hubspot-crm, intercom, mailgun, mastodon, mediawiki-action-api, nasa-neows, postmark, quickbooks-online, sentry, strava, usgs-earthquake-catalog, xero-accounting | 20 | examples/digitalocean-sdk/FIRST_USE.md:20; examples/arxiv-sdk/FIRST_USE.md:11; examples/elasticsearch-sdk/FIRST_USE.md:146 |
| H2 | `validate_header_names()` leads the generated client page (or costs a test) although no native header names exist | arxiv, europe-pmc, kraken-spot-rest, square-payments, stripe, usgs-earthquake-catalog | 6 | examples/europe-pmc-sdk/FIRST_USE.md:211; examples/usgs-earthquake-catalog-sdk/FIRST_USE.md:174; examples/kraken-spot-rest-sdk/FIRST_USE.md:170 |
| H3 | Generated docs are long and repeat framework prose (format/send statements, `prepare` vs `send`, `for<'__concord>` bounds), burying method, path and inputs | arxiv, digitalocean, europe-pmc, kraken-spot-rest, square-payments, usgs-earthquake-catalog | 6 | examples/digitalocean-sdk/FIRST_USE.md:223; examples/europe-pmc-sdk/FIRST_USE.md:221; examples/arxiv-sdk/FIRST_USE.md:195 |
| H4 | Generated docs hide concrete wire facts (Accept value, pinned query values, `QueryPairs` keys, Bearer prefix, an argument's wire role) *(docs-only fix: europe-pmc)* | adyen-checkout, arxiv, digitalocean, europe-pmc, fred, mailgun | 6 | examples/fred-sdk/FIRST_USE.md:22; examples/digitalocean-sdk/FIRST_USE.md:227; examples/mailgun-sdk/FIRST_USE.md:224 |
| H5 | Concord guides (RECIPES, CHEATSHEET, CUSTOM_FORMATS) give outdated native-route advice that contradicts current rustdoc | frankfurter, google-drive, nager-holidays, slack, twilio | 5 | examples/slack-sdk/FRICTION.md:61; examples/nager-holidays-sdk/FRICTION.md:54; examples/frankfurter-sdk/FRICTION.md:37 |
| H6 | Generated docs ignore the send policy (policy value "sent normally", generic `with_base_url` examples the policy refuses, "decoded from a 2xx", garbled policy sentence) | adyen-checkout, arxiv, kraken-spot-rest, paypal, usgs-earthquake-catalog | 5 | examples/arxiv-sdk/FIRST_USE.md:198; examples/paypal-sdk/FIRST_USE.md:139; examples/kraken-spot-rest-sdk/FIRST_USE.md:187 |
| H7 | Generated docs misrender the request (production default base as the URL, required query under "when present", `{issue.issue}` placeholders) | frankfurter, quickbooks-online, sentry | 3 | examples/quickbooks-online-sdk/FIRST_USE.md:162; examples/frankfurter-sdk/FIRST_USE.md:176; examples/sentry-sdk/FIRST_USE.md:245 |
| H8 | Rustdoc of re-exported types exposes the hidden declaration (placeholder host, hidden `Api`, `set_*`/`maybe_*`) | dropbox, mastodon | 2 | examples/mastodon-sdk/FRICTION.md:45; examples/dropbox-sdk/FIRST_USE.md:179 |
| H9 | No guidance on `#[non_exhaustive]` for SDK error enums and models; adding variants breaks callers | digitalocean, strava | 2 | examples/digitalocean-sdk/MAINTENANCE.md:73; examples/strava-sdk/MAINTENANCE.md:66 |
| H10 | Concord rustdoc built with default features omits feature-gated items (`Form`, csv, stream) | twilio, usgs-earthquake-catalog | 2 | examples/twilio-sdk/MAINTENANCE.md:66; examples/usgs-earthquake-catalog-sdk/FIRST_USE.md:189 |
| H11 | Naming a format type in an endpoint doc makes `rustdoc -D warnings` fail inside `api!` (generated explicit `[F](F)` links) | europe-pmc | 1 | examples/europe-pmc-sdk/MAINTENANCE.md:158 |
| H12 | `#[non_exhaustive]` `PagesError` variants say they "cannot be matched" although `{ .. }` matching works | square-payments | 1 | examples/square-payments-sdk/FIRST_USE.md:196 |
| H13 | Re-exported concord-auth items show author-facing docs and impossible variants on the SDK page | square-payments | 1 | examples/square-payments-sdk/FIRST_USE.md:193 |
| H14 | AUTHOR_GUIDE's method-based uncertainty example does not warn that later endpoints with the same method join the rule | square-payments | 1 | examples/square-payments-sdk/MAINTENANCE.md:205 |
| H15 | An SDK extension terminal is visible only under rustdoc "Blanket Implementations" | nasa-neows | 1 | examples/nasa-neows-sdk/MAINTENANCE.md:170 |
| H16 | `Incoming::chunk` cancel safety is undocumented | ntfy | 1 | examples/ntfy-sdk/FRICTION.md:92 |
| H17 | No guide recipe for a typed `Display` query enum | open-library | 1 | examples/open-library-sdk/MAINTENANCE.md:151 |
| H18 | No guide example of 404 as a typed success body and 409 as a typed rejection | elasticsearch | 1 | examples/elasticsearch-sdk/MAINTENANCE.md:147 |
| H19 | Guides do not say whether inherent impls on generated types are a supported extension point | gitlab | 1 | examples/gitlab-sdk/FRICTION.md:76 |
| H20 | AUTHOR_GUIDE header-template example lacks the optional value that omits the header | xero-accounting | 1 | examples/xero-accounting-sdk/MAINTENANCE.md:65 |
| H21 | Guides do not warn that `Json` accepts a JSON array for a struct (serde positional) | mastodon | 1 | examples/mastodon-sdk/FRICTION.md:136 |
| H22 | Format guide does not say an empty body is sent without `Content-Length` | google-drive | 1 | examples/google-drive-sdk/FRICTION.md:122 |
| H23 | `Decoder<T>` is `Copy`, which the concord-auth recipe does not lead readers to expect | quickbooks-online | 1 | examples/quickbooks-online-sdk/FRICTION.md:113 |

## I. Values and inputs

| # | Friction (one line) | SDKs | Count | Evidence |
|---|---|---|---|---|
| I1 | `ToHeaderValue` is sealed: domain or validated newtypes (including credentials) cannot be declared header values; SDK uses templates or a raw `HeaderValue` | amazon-s3, anthropic-messages, clickhouse, dropbox, github, gitlab, ntfy, rest-countries, stripe | 9 | examples/stripe-sdk/FRICTION.md:80; examples/github-sdk/FRICTION.md:134; examples/rest-countries-sdk/FRICTION.md:34 |
| I2 | Provider input constraints are not declarable; validated newtypes, input-error variants and restated rules are hand-written | datadog-events, mailgun, mastodon, open-library, paypal, rest-countries, spotify-web-api, usgs-earthquake-catalog | 8 | examples/mailgun-sdk/MAINTENANCE.md:84; examples/usgs-earthquake-catalog-sdk/FRICTION.md:11; examples/mastodon-sdk/MAINTENANCE.md:36 |
| I3 | Sealed `IntoValue` blocks inference (`Default::default()`, `.parse()`), rejects `&str`/`From` conversions, and differs between `set_` and `maybe_` | anthropic-messages, cloudflare-zones, frankfurter, kraken-spot-rest, square-payments | 5 | examples/kraken-spot-rest-sdk/FIRST_USE.md:175; examples/frankfurter-sdk/FRICTION.md:16; examples/cloudflare-zones-sdk/FRICTION.md:47 |
| I4 | Bodies are opaque whole values: one field cannot be edited, defaulted or made constant, and body options cannot get setters like query values | anthropic-messages, intercom, kraken-spot-rest, paypal | 4 | examples/intercom-sdk/MAINTENANCE.md:160; examples/anthropic-messages-sdk/FRICTION.md:119; examples/paypal-sdk/FRICTION.md:104 |
| I5 | Partial-update and omitted-vs-empty body semantics (`Patch<T>`, Option struct + builder + Debug) are hand-written | strava, stripe | 2 | examples/strava-sdk/MAINTENANCE.md:106; examples/stripe-sdk/FRICTION.md:19 |
| I6 | SDK filter/input records mirror declared parameters and are projected by hand; an omitted field still compiles | github, usgs-earthquake-catalog | 2 | examples/github-sdk/FRICTION.md:190; examples/usgs-earthquake-catalog-sdk/MAINTENANCE.md:91 |
| I7 | Generated endpoint bodies are owned with no lifetime, so shared input is cloned into each request | anthropic-messages | 1 | examples/anthropic-messages-sdk/MAINTENANCE.md:58 |
| I8 | No exact-decimal serde helper for bodies; amounts serialized by hand | xero-accounting | 1 | examples/xero-accounting-sdk/FRICTION.md:57 |
| I9 | No public UTC calendar/format helper (concord-retry has the inverse privately) | xero-accounting | 1 | examples/xero-accounting-sdk/MAINTENANCE.md:167 |

## J. Observability and logging

| # | Friction (one line) | SDKs | Count | Evidence |
|---|---|---|---|---|
| J1 | No logging convention: SDK error `Debug` shows provider text while `Display` redacts; safe logging depends on picking the right formatter | distribution-registry | 1 | examples/distribution-registry-sdk/FIRST_USE.md:57 |

## K. Domain facts and request composition

| # | Friction (one line) | SDKs | Count | Evidence |
|---|---|---|---|---|
| K1 | Percent-encoding has no per-value control: `:`, `+`, `,` and brackets escaped, `+` vs `%20`; SDK rewrites targets natively | amazon-s3, datadog-events, distribution-registry, rest-countries, usgs-earthquake-catalog | 5 | examples/rest-countries-sdk/FRICTION.md:15; examples/distribution-registry-sdk/FRICTION.md:79; examples/amazon-s3-sdk/FRICTION.md:68 |
| K2 | Query order follows declaration slots, not setter order; fixed keys land after arguments and runtime pairs mid-query (an observation more than a cost: World Bank calls it easy and Stripe found it documented; it surprised MediaWiki's first-use reader) | mediawiki-action-api, stripe, world-bank | 3 | examples/mediawiki-action-api-sdk/FIRST_USE.md:188; examples/stripe-sdk/FRICTION.md:193; examples/world-bank-sdk/FRICTION.md:3 |
| K3 | A path value is always one segment; multi-segment names (repository, object key, `/works/..` ids) need native rewrites or hand normalization | amazon-s3, distribution-registry, open-library | 3 | examples/amazon-s3-sdk/FRICTION.md:49; examples/distribution-registry-sdk/FRICTION.md:54; examples/open-library-sdk/FRICTION.md:67 |
| K4 | Keys or header names chosen at run time (`param_<name>`, `x-amz-meta-{name}`, `X-Bz-Info-<name>`) cannot be declared | amazon-s3, backblaze-b2-native, clickhouse | 3 | examples/clickhouse-sdk/FRICTION.md:18; examples/amazon-s3-sdk/FRICTION.md:82; examples/backblaze-b2-native-sdk/FRICTION.md:61 |
| K5 | Mixing API versions in one client forces the version segment into every endpoint path | datadog-events | 1 | examples/datadog-events-sdk/MAINTENANCE.md:113 |
| K6 | Runtime `query(..)` pairs can repeat a declared fixed key; Concord neither refuses nor exposes the declared keys | mediawiki-action-api | 1 | examples/mediawiki-action-api-sdk/FRICTION.md:26 |
| K7 | Empty-segment check is skipped for a value inside a templated segment (`{sid}.json`) | twilio | 1 | examples/twilio-sdk/FIRST_USE.md:196 |
| K8 | A GET with a body is neither refused nor documented | elasticsearch | 1 | examples/elasticsearch-sdk/FRICTION.md:117 |
| K9 | A fixed header literal and an SDK constant restate the same version fact | square-payments | 1 | examples/square-payments-sdk/MAINTENANCE.md:175 |
| K10 | Undeclared requests cannot reuse a scope's composed headers and segment encoding; SDK mirrors the declaration | github | 1 | examples/github-sdk/FRICTION.md:122 |

## L. Declaration syntax and conventions

| # | Friction (one line) | SDKs | Count | Evidence |
|---|---|---|---|---|
| L1 | The same statements are repeated on sibling endpoints (headers, sensitive query values, limit, stream format, fixed query block, per-request override); no shared endpoint-level block | amazon-s3, anthropic-messages, kraken-spot-rest, kubernetes, mastodon, mediawiki-action-api, paypal, stripe | 8 | examples/kubernetes-sdk/FRICTION.md:8; examples/stripe-sdk/FRICTION.md:62; examples/paypal-sdk/MAINTENANCE.md:147 |
| L2 | Parameter groups cannot hold required values or SDK methods, and an omitted `..Group` still compiles | adyen-checkout, arxiv, fred | 3 | examples/adyen-checkout-sdk/MAINTENANCE.md:158; examples/arxiv-sdk/MAINTENANCE.md:126; examples/fred-sdk/MAINTENANCE.md:92 |
| L3 | Compile errors land on the `api!`/client `send` line or name Concord internals (`__private::BodyFormat`) instead of the endpoint at fault | adyen-checkout, hubspot-crm, postmark | 3 | examples/adyen-checkout-sdk/MAINTENANCE.md:136; examples/postmark-sdk/MAINTENANCE.md:120; examples/hubspot-crm-sdk/MAINTENANCE.md:109 |
| L4 | An endpoint declared only to build a URL (OAuth authorize) still gets a generated `send()` that must be hidden | spotify-web-api | 1 | examples/spotify-web-api-sdk/FRICTION.md:114 |
| L5 | A scope has exactly one parent, so a shared sub-scope is declared once per API group | kubernetes | 1 | examples/kubernetes-sdk/FRICTION.md:72 |
| L6 | A scope path cannot end in `/`, so endpoints repeat `{scope}/` | sentry | 1 | examples/sentry-sdk/FRICTION.md:47 |
| L7 | rustfmt does not format `api!` bodies | stripe | 1 | examples/stripe-sdk/FRICTION.md:195 |

## M. Testing story for SDK authors

| # | Friction (one line) | SDKs | Count | Evidence |
|---|---|---|---|---|
| M1 | testserver cannot simulate some wire behaviour (declared-length premature EOF, hold a reply until another request, client close, record a failed streamed body, https/subdomain hosts) | amazon-s3, datadog-events, dropbox, kubernetes, mailgun | 5 | examples/amazon-s3-sdk/FIRST_USE.md:100; examples/dropbox-sdk/FRICTION.md:86; examples/kubernetes-sdk/FRICTION.md:209 |
| M2 | Fixture servers, token-route stubs and routers are rebuilt in each SDK test binary | hubspot-crm, intercom, quickbooks-online, salesforce | 4 | examples/salesforce-sdk/FRICTION.md:199; examples/hubspot-crm-sdk/MAINTENANCE.md:124; examples/intercom-sdk/MAINTENANCE.md:152 |
| M3 | testserver `Reply` is not `Clone`; table-driven tests need factories | hubspot-crm, sentry | 2 | examples/hubspot-crm-sdk/MAINTENANCE.md:129; examples/sentry-sdk/FIRST_USE.md:250 |
| M4 | Scripted replies cannot know the server URL, so absolute `Location`/`Link` replies need workarounds | distribution-registry, gitlab | 2 | examples/distribution-registry-sdk/FRICTION.md:168; examples/gitlab-sdk/FRICTION.md:117 |
| M5 | testserver `Body::Chunks` takes `bytes::Bytes`, which is not re-exported; `Reply::json` adds a charset | frankfurter, usgs-earthquake-catalog | 2 | examples/frankfurter-sdk/FIRST_USE.md:188; examples/usgs-earthquake-catalog-sdk/FIRST_USE.md:203 |
| M6 | testserver is under-documented (no README; chunk delivery semantics) | intercom, openai | 2 | examples/intercom-sdk/FIRST_USE.md:12; examples/openai-sdk/MAINTENANCE.md:37 |
| M7 | testserver writes head and body separately without `TCP_NODELAY` (~40 ms per small response) | kubernetes | 1 | examples/kubernetes-sdk/FRICTION.md:203 |

## N. Removal and simplification

No SDK note reports a Concord item as removable. cross50's removal table (`first_page_then`, `Order::Completion`, `Options::failure_limit`, `Error::meta()`, `concord-extras`) comes from code sweeps with 0 SDK reports (`v4/sdk50/cross50/PROPOSALS.md:111`), so it is not counted here.

## O. Send policy, auth, retry and rate-limit capabilities (new family)

| # | Friction (one line) | SDKs | Count | Evidence |
|---|---|---|---|---|
| O1 | concord-ratelimit does not model provider quotas (response-named, sliding, multi-window, keyed buckets, unknown reset); quota headers parsed and mapped by hand | cloudflare-zones, digitalocean, discord-rest, hubspot-crm, intercom, nasa-neows, sentry, spotify-web-api, strava | 9 | examples/sentry-sdk/FRICTION.md:17; examples/strava-sdk/FRICTION.md:11; examples/cloudflare-zones-sdk/FRICTION.md:76 |
| O2 | `prepare()` / `Prepared::send()` silently bypass the client send policy (auth, signing, admission, error mapping) and generated docs do not say so | adyen-checkout, arxiv, elasticsearch, open-library, quickbooks-online, spotify-web-api, strava | 7 | examples/strava-sdk/FRICTION.md:16; examples/open-library-sdk/FRICTION.md:28; examples/adyen-checkout-sdk/FRICTION.md:39 |
| O3 | concord-auth `TokenCache` holds only header + lifetime: refresh-token rotation, its expiry and durable persistence live in SDK state | hubspot-crm, quickbooks-online, spotify-web-api, strava, xero-accounting | 5 | examples/xero-accounting-sdk/FRICTION.md:5; examples/spotify-web-api-sdk/FRICTION.md:33; examples/hubspot-crm-sdk/FRICTION.md:8 |
| O4 | concord-retry's sync/infallible `receive` hook cannot await a token or admission or refuse an attempt, so SDKs hand-wire `Retry::run` *(cross50 PROPOSALS A4: existing `receive_with` covers adyen/xero; the selection rule is undocumented)* | adyen-checkout, paypal, sentry, xero-accounting | 4 | examples/adyen-checkout-sdk/FRICTION.md:7; examples/paypal-sdk/FRICTION.md:27; examples/sentry-sdk/FRICTION.md:27 |
| O5 | concord-retry reports misstate what happened (attempts counted at factory start, no completed/abandoned count, `Deadline` after a `Done` head) | adyen-checkout, backblaze-b2-native, paypal, sentry | 4 | examples/backblaze-b2-native-sdk/FRICTION.md:111; examples/adyen-checkout-sdk/FRICTION.md:20; examples/paypal-sdk/FRICTION.md:43 |
| O6 | concord-retry's Retry-After helpers fall short (needs a wall clock, rejects fractional seconds, hint not carried to the final outcome, parser-only use undocumented) | anthropic-messages, discord-rest, intercom, zendesk-support | 4 | examples/anthropic-messages-sdk/FRICTION.md:101; examples/discord-rest-sdk/FRICTION.md:83; examples/zendesk-support-sdk/FRICTION.md:48 |
| O7 | concord-auth has no constructor for a raw API-key header (Anthropic `x-api-key`), and leaves payload signatures to the SDK (Kraken); Basic is covered by `concord_auth::basic` (Twilio uses it) | anthropic-messages, kraken-spot-rest | 2 | examples/anthropic-messages-sdk/FRICTION.md:88; examples/kraken-spot-rest-sdk/FRICTION.md:70 |
| O8 | `TokenCache` cannot be seeded with an already-issued access token | quickbooks-online, spotify-web-api, strava | 3 | examples/quickbooks-online-sdk/FRICTION.md:66; examples/spotify-web-api-sdk/FRICTION.md:50; examples/strava-sdk/FRICTION.md:10 |
| O9 | A followed `Prepared` loses the client send policy; continuation pages re-apply the policy by hand | digitalocean, mailgun, zendesk-support | 3 | examples/digitalocean-sdk/FRICTION.md:10; examples/mailgun-sdk/FRICTION.md:13; examples/zendesk-support-sdk/FRICTION.md:74 |
| O10 | concord-retry classifiers see only the unread head; body-borne error codes or a new target per attempt force `Retry::run` | backblaze-b2-native, spotify-web-api | 2 | examples/backblaze-b2-native-sdk/FRICTION.md:103; examples/spotify-web-api-sdk/FRICTION.md:84 |
| O11 | Retry deadline ends at the final response head; bounding the final body decode is hand-written | sentry, spotify-web-api | 2 | examples/sentry-sdk/FRICTION.md:38; examples/spotify-web-api-sdk/MAINTENANCE.md:60 |
| O12 | `Run::outcome` is an `Option` whose unreachable `None` forces an extra SDK error; `receive` needs many plumbing arguments | cloudflare-zones, zendesk-support | 2 | examples/cloudflare-zones-sdk/FRICTION.md:64; examples/zendesk-support-sdk/FRICTION.md:43 |
| O13 | Composing retry, token refresh and rate-limit admission (lease across backoff) in one policy is hand-written and undocumented | strava, xero-accounting | 2 | examples/strava-sdk/FRICTION.md:12; examples/xero-accounting-sdk/FRICTION.md:35 |
| O14 | `TokenCache` forgets failed acquisitions; a terminally rejected grant is re-requested unless the SDK remembers it | quickbooks-online, spotify-web-api | 2 | examples/quickbooks-online-sdk/FRICTION.md:65; examples/spotify-web-api-sdk/FRICTION.md:75 |
| O15 | `TokenCache` shares an acquisition failure as `Arc<E>`; the SDK cannot own the error | paypal, spotify-web-api | 2 | examples/spotify-web-api-sdk/FRICTION.md:68; examples/paypal-sdk/FRICTION.md:130 |
| O16 | A send-policy value cannot have a default, so every caller fills a policy field | kraken-spot-rest, zendesk-support | 2 | examples/kraken-spot-rest-sdk/FIRST_USE.md:173; examples/zendesk-support-sdk/FRICTION.md:115 |
| O17 | No per-endpoint post-decode check hook; SDK adds a policy trait bound with one impl per output (`Confirm`, `Reply`) | adyen-checkout, postmark | 2 | examples/postmark-sdk/MAINTENANCE.md:101; examples/adyen-checkout-sdk/MAINTENANCE.md:64 |
| O18 | Policy-applied request decoration (post-encoding signature, auth header) has no declared slot or seam; done natively after `into_parts` | kraken-spot-rest, paypal | 2 | examples/kraken-spot-rest-sdk/FRICTION.md:7; examples/paypal-sdk/FRICTION.md:72 |
| O19 | A Bearer challenge (RFC 7235) cannot be answered by a send policy (endpoint not re-preparable); `WWW-Authenticate` parsed by hand | distribution-registry | 1 | examples/distribution-registry-sdk/FRICTION.md:103 |
| O20 | concord-ratelimit shares one FIFO queue across keys; a blocked key delays unrelated endpoints | sentry | 1 | examples/sentry-sdk/FRICTION.md:65 |
| O21 | Limiter windows take monotonic deadlines; provider wall-clock resets are converted by hand | sentry | 1 | examples/sentry-sdk/FRICTION.md:18 |
| O22 | `TokenCache` clock cannot be read back; SDK keeps a duplicate clock | spotify-web-api | 1 | examples/spotify-web-api-sdk/FRICTION.md:60 |
| O23 | `TokenCache` has no minimum remaining validity option | paypal | 1 | examples/paypal-sdk/FRICTION.md:134 |
| O24 | `TokenCache` must not be captured by its own acquisition closure (Arc cycle), so connection state is split by hand | quickbooks-online | 1 | examples/quickbooks-online-sdk/FRICTION.md:68 |
| O25 | A refresh-token replacement during an in-flight refresh cannot win over the flight's result via `TokenCache` alone | quickbooks-online | 1 | examples/quickbooks-online-sdk/FRICTION.md:67 |
| O26 | Per-operation OAuth scope requirements and granted-scope parsing are hand-written; scopes are not declarable | strava | 1 | examples/strava-sdk/MAINTENANCE.md:104 |
| O27 | No per-send generated value (nonce); an endpoint default is evaluated at construction and kept by clones | kraken-spot-rest | 1 | examples/kraken-spot-rest-sdk/FRICTION.md:21 |
| O28 | Generated client has `set_policy` but no policy getter | kraken-spot-rest | 1 | examples/kraken-spot-rest-sdk/FIRST_USE.md:213 |
| O29 | concord-retry has one `max_attempts` for every cause; per-cause budgets are counted by hand | spotify-web-api | 1 | examples/spotify-web-api-sdk/FRICTION.md:98 |
| O30 | No one-attempt send helper beside concord-retry; the write path duplicates token/execute/401 handling | spotify-web-api | 1 | examples/spotify-web-api-sdk/MAINTENANCE.md:117 |
| O31 | concord-retry cannot make the next attempt depend on the previous outcome (shrinking bulk resubmission) | elasticsearch | 1 | examples/elasticsearch-sdk/FRICTION.md:62 |
| O32 | No bounded polling helper for async job receipts | strava | 1 | examples/strava-sdk/FRICTION.md:13 |

## P. Formats, codecs and bodies (new family)

| # | Friction (one line) | SDKs | Count | Evidence |
|---|---|---|---|---|
| P1 | Response format or output type is fixed per endpoint: a per-call representation (JSON/XML/CSV/ZIP), runtime CSV dialect or caller-chosen record type needs twin endpoints or hand dispatch | clickhouse, europe-pmc, fred, open-meteo, salesforce | 5 | examples/open-meteo-sdk/FRICTION.md:28; examples/europe-pmc-sdk/FRICTION.md:14; examples/clickhouse-sdk/FRICTION.md:70 |
| P2 | Stream `ReadError` lacks kinds and accessors (truncation, terminal vs skippable, `None` after error looks like EOF, recovering `concord::Error`) | anthropic-messages, mastodon, ntfy, shopify | 4 | examples/mastodon-sdk/FRICTION.md:81; examples/ntfy-sdk/FIRST_USE.md:143; examples/shopify-sdk/FRICTION.md:142 |
| P3 | No XML format in concord-codecs; namespace-aware XML `Decode` impls and parser limits are hand-written | arxiv, europe-pmc, fred | 3 | examples/arxiv-sdk/FRICTION.md:9; examples/europe-pmc-sdk/FRICTION.md:8; examples/fred-sdk/FRICTION.md:9 |
| P4 | No streamed multipart (or no multipart encoder at all); SDKs build multipart bodies by hand *(google-drive excluded: cross50 a6 says it now uses shared related multipart)* | mailgun, openai, strava | 3 | examples/mailgun-sdk/FRICTION.md:11; examples/openai-sdk/FRICTION.md:7; examples/strava-sdk/FRICTION.md:9 |
| P5 | Csv codec cannot express provider CSV variants (two tables in one document, required-column sets, strict line endings) | open-meteo, salesforce, usgs-earthquake-catalog | 3 | examples/open-meteo-sdk/FRICTION.md:49; examples/usgs-earthquake-catalog-sdk/FRICTION.md:12; examples/salesforce-sdk/FRICTION.md:110 |
| P6 | Content-Type helpers fall short (private parsing for non-JSON readers, which media type matched, CSV writer forcing `header=present`) | ntfy, salesforce, zendesk-support | 3 | examples/ntfy-sdk/FRICTION.md:58; examples/zendesk-support-sdk/FRICTION.md:97; examples/salesforce-sdk/FRICTION.md:72 |
| P7 | JSONL support gaps (stops at the first bad row, no `check_head`, `DeserializeOwned` bound, one record type per batch) | clickhouse, elasticsearch, openai | 3 | examples/openai-sdk/FRICTION.md:9; examples/clickhouse-sdk/FRICTION.md:47; examples/elasticsearch-sdk/FRICTION.md:27 |
| P8 | No bounded or exact-length request body (one-shot stream with exact `Content-Length`, bounded writers) | backblaze-b2-native, elasticsearch, salesforce | 3 | examples/backblaze-b2-native-sdk/FRICTION.md:73; examples/salesforce-sdk/FRICTION.md:83; examples/elasticsearch-sdk/MAINTENANCE.md:144 |
| P9 | A reader cannot take client configuration (limits) at open; bounds are crate constants or static `Decode::context` *(cross50: configured readers already expose `read_for`, so ntfy's case is a recipe gap)* | anthropic-messages, ntfy, snowflake | 3 | examples/anthropic-messages-sdk/FRICTION.md:41; examples/ntfy-sdk/FRICTION.md:70; examples/snowflake-sdk/FRICTION.md:128 |
| P10 | Body limits are hard to use: no per-request override, finite and incremental limits on separate paths, bounded raw bytes without metadata *(nasa and twilio excluded: cross50 a3 says the declared `limit` now covers success bodies; europe-pmc covered by `document::<Binary, Bytes>`)* | elasticsearch, europe-pmc, usgs-earthquake-catalog | 3 | examples/elasticsearch-sdk/FRICTION.md:97; examples/usgs-earthquake-catalog-sdk/FRICTION.md:16; examples/europe-pmc-sdk/FRICTION.md:11 |
| P11 | No SSE reader; SSE framing, split UTF-8 handling and completion are hand-written | ntfy, openai | 2 | examples/ntfy-sdk/FRICTION.md:15; examples/openai-sdk/FRICTION.md:8 |
| P12 | Stream reader vocabulary is closed (crate-private `ReadError`/`OpenError` constructors, sealed `RecordReader`); raw downloads and custom readers re-create it | github, ntfy | 2 | examples/github-sdk/FRICTION.md:146; examples/ntfy-sdk/FRICTION.md:37 |
| P13 | No GraphQL binding (document, variables, result); variables cannot be placed inside a body | linear, shopify | 2 | examples/linear-sdk/FRICTION.md:6; examples/shopify-sdk/FRICTION.md:8 |
| P14 | No bounded `Content-Encoding` (gzip) decoding; reqwest's workspace-unified decompression has to be switched off by hand | snowflake | 1 | examples/snowflake-sdk/FRICTION.md:108 |
| P15 | `concord::Form` is flat; bracket-convention nested forms need a custom serializer | stripe | 1 | examples/stripe-sdk/FRICTION.md:15 |
| P16 | `Text` codec encodes only `String` | ntfy | 1 | examples/ntfy-sdk/FRICTION.md:125 |
| P17 | A runtime-media format must stub the unusable `Encode::encode` | mailgun | 1 | examples/mailgun-sdk/FRICTION.md:12 |
| P18 | Native multipart fallback encodes field names differently from the codec, uncaught by the compiler | mailgun | 1 | examples/mailgun-sdk/MAINTENANCE.md:100 |
| P19 | No bounded ZIP/archive reader | fred | 1 | examples/fred-sdk/FRICTION.md:10 |
| P20 | SSE comments/heartbeats are invisible and `position()` behaviour over them is undocumented | mastodon | 1 | examples/mastodon-sdk/FRICTION.md:93 |
| P21 | A response format sees only the response head, not the request (206 vs requested range) | amazon-s3 | 1 | examples/amazon-s3-sdk/FRICTION.md:98 |
| P22 | Labelling a native response via `Incoming::new` turns a non-2xx into an error before the reader's own check | shopify | 1 | examples/shopify-sdk/FRICTION.md:100 |
| P23 | A byte filter in front of a record reader bypasses `open()` checks and loses the operation name | clickhouse | 1 | examples/clickhouse-sdk/FRICTION.md:36 |
| P24 | Finite JSON decode buffers the whole body; an interrupted array yields no partial items | nager-holidays | 1 | examples/nager-holidays-sdk/FIRST_USE.md:26 |
| P25 | Csv decoder accepts an unterminated quoted field at EOF | open-meteo | 1 | examples/open-meteo-sdk/FRICTION.md:69 |

## Most-reported frictions overall

| Rank | # | Friction | Count |
|---|---|---|---|
| 1 | A1 | Provider error document cannot be declared: SDK hand-writes the status branch, bounded error-body read, typed error document and its own error type (From/Display/accessors) around `concord::Error` | 38 |
| 2 | H1 | SDK rustdoc built with `--no-deps` leaves Concord types unlinked; consumers build Concord crates' docs separately to learn errors, metadata and pagination | 20 |
| 3 | C1 | Safe transport (redirects off, native retries off, timeouts) is built by hand in each SDK; Concord cannot check or enforce the supplied reqwest client | 19 |
| 4 | B1 | No core value-plus-metadata result: SDK defines its own `Response<T>` and captures the response head by hand | 18 |
| 5 | D1 | SDK-owned models, errors, cursors and inputs need hand-written redacting `Debug`/`Display`; a derived `Debug` compiles and leaks | 18 |
| 6 | F1 | Paging cannot be declared: each paged operation needs a hand adapter (clone context into a page factory, rebuild the endpoint, project cursor or next link from body/header into `Page`) | 17 |
| 7 | A2 | Generated rustdoc prints bare `Error` for `concord::Error`, the SDK error and capability-crate errors alike; callers cannot tell which one they hold | 16 |
| 8 | E1 | Declared values are not readable: the send policy sees only the built request and handles have no getters, so SDKs re-parse values from the prepared URL, headers or body by name | 16 |
| 9 | G1 | Concord types callers need (`ErrorKind`, pagination, retry/ratelimit/auth payloads, `Bytes`) are not re-exported where callers look; apps add version-matched direct dependencies | 13 |
| 10 | F2 | concord-pagination vocabulary is hard to learn from SDK docs (extra `Resume` layer, `Incomplete.collected` vs `items`, `Items` vs `PageItems`, `None` after an error, missing aliases) | 12 |
| 11 | A3 | Provider failure inside a 2xx body (`ok:false`, `success:false`, `errors` beside data, Atom error feed, trailing exception block) cannot be declared; SDK detects it by hand | 11 |
| 12 | E2 | "Not sent" vs "maybe applied" for a write is classified by hand from `Delivery`, reqwest's error taxonomy and the method | 11 |
| 13 | G2 | Hand-written forwarders / domain methods over generated endpoints (to return SDK results, hide the client, add paging or typed lookups) | 11 |
| 14 | F3 | Pagination does not refuse a repeated or non-progressing cursor; SDK keeps its own seen-set or relies on caps | 10 |
| 15 | B2 | Generated `send()` rustdoc shows a projection (`<P as SendWith<T>>::Output`, `Records<F, I>`, wire `Envelope<T>`) instead of the concrete result or reader type | 9 |
Also at 9 (tied with B2): E3, F4, I1, O1.

## Corrections after review

`p3-inventory-review-1` (Codex `gpt-6.1-sol`, 10 entries sampled): A4 no longer says the compiler misses an omitted arm; D3 covers query placement too; O7 drops Twilio and Basic (now in `concord-auth`), count 3 → 2; K2 marked as an observation. The previous text is `FRICTION.v1.md`.
