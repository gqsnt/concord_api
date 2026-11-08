# H06 design survey — construction, controls and scope representation (base `a883f7bb`)

Read-only survey by a Claude subagent (Explore, claude-opus-5-5) for the coordinator; not a review
or a decision. (obs) = cited from source, docs or tests; (inf) = the surveyor's inference; nothing
was compiled — statements about compiler behaviour are expectations ("Expect") unless a test is
cited. Paths relative to the repository; `ex/` = `examples/`. Boundary: one recursive `*.md` grep
traversed `tools/desirability/` with its output path-filtered (no line from it appeared; transcript
audited: 0 hits in tool results); nothing under `work/sealed` or `work/baseline` touched.

## 1. What the macro generates today (obs)
- **One visibility for everything:** the client's `vis` (syntax.rs:712, model.rs:119-131) applies to
  every generated item (emit.rs:16-43): client and `{Client}Config` (1670), scope and endpoint types
  (1694, 1770), the `operations` enum. Every method is unconditionally `pub fn` (controls 887, 910,
  928, 935, 945; `new` 1711; `with_base_url` 1732; navigation 2337, 2371). Values accept only docs
  and `#[sensitive]`; scopes/endpoints also `#[cfg]`/`#[deprecated]` (2320-2322). Hiding is
  all-or-nothing per declaration (`pub(crate)` client or a private `mod`) (inf).
- **Client (1607-1752):** `{Client}Config` only with required values, `pub` fields of exact `T`,
  redacting `Debug`; `new(http[, config])` infallible (base checked at compile time);
  `with_base_url` replaces only the base and keeps every value, credentials included (1733-1734);
  `set_x`/`maybe_x` for every value, required ones included (849-953); `base` mandatory and
  client-only; `send` client-only; no value named `http`.
- **Scopes (1753-1796):** no constructor; entered by `parent.scope(required…)` positionally; every
  scope value gets `set_*`/`maybe_*`; `set_tenant("beta")` on a scope keeps the client key
  (`crates/concord/tests/values.rs:440-468`).
- Endpoints: value/body controls (1813-1820, 917-951); parameter-group traits carry `set_`/`maybe_`.
  Handles are copy-on-write `Arc` snapshots; a setter returns a new handle; no getter for any value.

## 2. Wrapper inventory (42 of 55 SDKs; classification by the surveyor, totals ±10%)
Unit: one public hand-written construction/control/navigation/re-exposure item. Classes (A1 §3):
**M** mechanical adaptation, **P** provider semantics, **S** deliberate safety boundary, **R**
unexplained residue. Exposure: **H** generated client not publicly nameable; **F** public generated
client + facade (**F\*** facade does not re-expose it); **G** public generated client + helpers.

| SDK | Exp | M/P/S/R | Items (class, file:line, reason) |
|---|---|---|---|
| adyen-checkout | G | 0/1/0/0 | P `Checkout::sandbox` lib:82 |
| airtable | H | 2/1/0/0 | P `Client` lib:291 (per-operation input rules); M `new` :296, `with_base_url` :307 |
| amazon-s3 | H | 5/5/0/0 | P `S3Client` client:147, `new` :162, `with_endpoint` :191, `prepare` :281, `sign` :292; M `with_credentials` :230, `with_clock` :240, `region`/`endpoint`/`bucket` :259-270 |
| arxiv | G | 1/0/0/0 | M `with_http` lib:71 |
| backblaze-b2 | H | 2/1/2/0 | S `Client` lib:56, `Session` :146 (key only to the account host; role tokens bound to issued hosts); P `with_base_url` :77; M `new` :63, `Session::account` :161 |
| clickhouse | H | 3/0/2/0 | S `Credentials` client:30-75 (sum type vs 3 independent optionals on the wire), `ClickHouse`+`new` :153/:160; M `CallContext` :94, `with_base_url` :187, `with_database` :196 |
| cloudflare | G | 1/0/0/0 | M `connect` lib:150 |
| datadog | F | 7/1/0/0 | P `DatadogEvents`+`new` client:48/:66 (site → host; generated `new` skips the checks); M `with_base_url` :103, `site` :109, `api` :114, `events` :119, `list`/`search`/`post` :127-141 |
| digitalocean | G | 1/0/0/1 | M `connect` lib:58; **R** `for_fixture` :71 (loopback-only, yet `with_base_url` stays public) |
| discord | G | 1/0/0/0 | M `anchor` api:110 |
| distribution-registry | H | 4/2/0/0 | P `Registry`+`new` client:50/:70, `token_auth` :95; M `base_url` :102, `repository` :112, `Repository` :347 (+accessors; a scope rebuilt as a second client) |
| dropbox | H | 3/2/0/0 | P `Client` lib:55, `RequestContext`; M `new` :62, `with_base_url` :70, Config re-export lib:17 |
| elasticsearch | F | 2/0/0/0 | M `Elasticsearch`+`new` lib:158/:164, `api` :176 |
| github | F | 15/3/0/0 | M copies kept in step with the generated client (context:114-195, `repo` :170, `RepoContext` :305-320, `issues`/`runs` :326/:356); P `is_api_url` :201, `request` :216/:388 |
| gitlab | G | 1/0/0/0 | M `with_token` api:111 |
| google-drive | H | 1/1/2/0 | S `DriveClient` client:45, `with_config` :72 (credentials bound to an HTTPS or loopback origin); P `ClientConfig` :15; M `new` :66 |
| hubspot-crm | F\* | 0/3/0/0 | P `HubSpot`+`new` lib:392/:403 (refresh; rebuilds `CrmApi` per call), `with_base_url` :431, `refresh_token` :441 |
| intercom | G | 1/0/0/0 | M `with_token` lib:106 |
| kubernetes | F | 13/2/0/0 | M `KubeContext` context:32-84 (constructors, renamed navigation), 5 `NamespaceContext` accessors :121-141; P `namespace` :89, `NamespaceContext` :111 |
| linear | F\* | 3/1/0/0 | P `Linear` lib:246; M `api_key` :251, `oauth` :255, `with_base_url` :270 |
| mailgun | G | 1/1/0/0 | P `connect` lib:101; M `local` :106 |
| mastodon | H | 7/1/5/0 | S `Mastodon`+`new` lib:124/:133, three stream constructors :197-224 (host must be the instance's own), `InstanceUrl` model:16-68; M `with_token` :145, `instance_url` :151, 4 forwarders :156-172, aliases :97-101; P `discover_streaming` :184 |
| microsoft-graph | H | 0/2/2/0 | S `GraphClient`+`new` client:63/:84, `for_loopback` :95; P `AuthContext` :18, `with_body_limits` :174 |
| notion | F | 4/0/1/0 | S `NotionClient` client:35 (completeness); M `new` :51, `with_base_url` :63, `with_version` :72, `api` :79 |
| ntfy | G | 1/0/0/0 | M `with_token` lib:169 |
| openai | F\* | 2/1/0/0 | P `OpenAi` lib:317; M `new` :321, `with_base_url` :329 |
| open-meteo | H | 2/1/0/0 | P `OpenMeteo` lib:48; M `new` :54, `with_base_url` :62 |
| paypal | G | 1/8/0/0 | P `builder` lib:185, `Builder` :201 and its methods :227-268; M `sandbox` :178 |
| quickbooks | G | 0/1/0/0 | P `connect` lib:153 (environment → base) |
| rest-countries | G | 0/4/0/0 | P typed shortcuts lib:110-131 |
| salesforce | F | 4/2/0/0 | P `OrgContext`+`new` context:41/:85 (instance URL; version ≥ MINIMUM :91-93), link validators :137-255; M `api` :112, accessors :117-127 |
| sentry | G | 1/0/0/0 | M `with_token` lib:84 |
| shopify | F | 11/1/0/0 | M `ShopifyContext` context:44-88, `ShopContext` accessors :120-135; P `ShopContext` :110 |
| slack | H | 3/1/0/0 | P `Client` lib:117 (`ok:false` envelope); M `new` :123, `with_base_url` :133, Config re-export :16 |
| snowflake | H | 0/1/1/0 | S `Client`+`new` client:69/:83 (account origin; token and token_type set together); P `ClientOptions` :37 |
| spotify | G | 0/2/0/0 | P `Session` options session.rs:75-95, `replace_grant` :286 |
| strava | G | 1/2/0/0 | P `Strava::connect` lib:189, `Session::connect` auth:126; M `Session::client` auth:142 |
| stripe | F | 9/0/0/0 | M `StripeContext` context:60-125, `AccountOverride`+`OnAccount` macro :158-211 |
| twilio | H | 4/1/0/0 | P `Twilio` lib:65; M `new` :75, `with_base_url` :97, `prepare_*` :128/:137 |
| usgs | G | 1/0/0/0 | M `query` lib:84 |
| xero | H | 1/1/8/0 | S `Xero` lib:61, `with_origins` :82, `tenant` :109, `Tenant` :140, tenant-bound operations :155-194; P `new` :72; M `Tenant::id` :151 |
| zendesk | G | 1/0/0/0 | M `connect` lib:115 |

No wrappers (13): anthropic, europe-pmc, frankfurter, fred, kraken, mediawiki, nager, nasa,
open-library, pokeapi, postmark, square, world-bank. **Totals: M 120 · P 53 · S 23 · R 1** (197
items); exposure H 14, F 11, G 17; plus 18 public transport recipes, 2 `redirect_policy`, private
transports in 7 constructors. 62 of the 120 M items sit in github 15, kubernetes 13, shopify 11,
stripe 9, datadog 7, mastodon 7. M causes (inf, ±5): ~50 forwarders/renames/re-exposure; ~30
copies/accessors kept because handles have no getters (europe-pmc MAINTENANCE:120, stripe F3);
~25 token→header or default-policy constructors; ~5 record→setters mappings.

## 3. Where generated construction and controls do not match intended use
**Witnesses.** *Mastodon* (instance + token): generated `with_base_url` keeps the token; `base`
mandatory → placeholder `https://mastodon.invalid` (api.rs:21-23); the SDK hides `Api` (api.rs:1-4,
lib.rs:88), re-exports endpoint types only; `new` binds the instance (lib.rs:133-140, an
`unreachable!` at :137); streams only on the advertised host (lib.rs:233-238; tests). FRICTION §1:
generated docs show the placeholder host and an unnameable client; proposed runtime `base;`.
*Xero* (tenant scope as a header): `#[sensitive] header("xero-tenant-id")` scope value (lib.rs:43)
would get `set_tenant`; the whole declaration is kept in `mod wire` (lib.rs:33-57, README:5);
FRICTION F2: publishing the scope "would allow a tenant header setter beside a policy bound to
another grant"; smallest proposed change: an opt-in generated API view omitting context/base setters;
removing the wrappers "rejected by the cross-tenant cases"; `tenant()` accepts only the grant's
tenant (lib.rs:109-117). *Issued host — Backblaze B2*: hosts issued per credential role
(wire.rs:1-7); the SDK clones the private client and calls `with_base_url` per role
(lib.rs:133-141); issued URLs validated (model.rs:398-413). *Dynamic host with a public generated
client — Salesforce*: placeholder base (api.rs:40); generated client public (lib.rs:65) and handed
out by `OrgContext::api()` (context.rs:112). Expect: `api().clone().with_base_url(x)` sends the token
to x (no origin check found); `set_version(v)` bypasses the MINIMUM check (context.rs:91-93);
`SalesforceApi::new` aims at `.invalid`. Other placeholder/localhost bases: snowflake, distribution,
elasticsearch, clickhouse.

**Contrary cases.** *QuickBooks*: one `Connection` value is path realm + send policy + token source
(lib.rs:55-73); `set_connection` replaces them together; a separate realm value was rejected
("set_realm(other) [would] send one realm's token to another realm's path", FRICTION:29-31). Open
(inf): generated `new` uses the production base whatever the connection's environment
(lib.rs:67 vs `connect` :153-158); `with_base_url` public without origin check. *Rotation works
everywhere*: `set_key` rotation keeps the base, earlier requests keep the old key (values.rs:440-468);
S3 `with_credentials` (client.rs:228-237); Kubernetes `with_bearer` and generated
`set_authorization`; Mastodon/ntfy `with_token`; Spotify `replace_grant`; QuickBooks
`replace_refresh_token`; HubSpot rebuilds per call; Graph has no replacement control. Cloudflare and
Zendesk document `set_policy` as the way to replace the policy (cloudflare policy.rs:63-64; zendesk
policy.rs:28).

**Public generated controls that break the SDK's own invariant (Expect unless tested).** Shopify:
shop-scope tokens are per shop (api.rs:35-40), type re-exported, `ShopContext::admin()` returns it
— `set_shop(b)` sends shop A's token to shop B (no test). Zendesk: `set_subdomain` keeps the token
(lib.rs:43-53; tested only with a malformed label). Credential stored apart from per-credential
state: Discord token + limiter ("one per bot token", api.rs:39-45), Sentry authorization + policy
(lib.rs:35-40), Adyen api_key + policy (lib.rs:29-33, 82-85). ClickHouse and Snowflake: nonsensical
combinations constructible on the generated client (hidden behind facades). Datadog facade
`with_base_url` leaves `site()` stale. DigitalOcean `for_fixture` loopback rule bypassed by public
`with_base_url`. Dropbox/Slack re-export the generated Config (AUTHOR_GUIDE:1829-1832).

**Valid designs that cannot be declared (obs).** `send` client-only (QuickBooks realm cannot be a
scope); `base` client-only and mandatory (placeholders, per-scope host clones); one-segment path
holes (Distribution's repository as a second client); `query(..)` endpoint-only with no header
analogue; `ToHeaderValue` sealed (token types converted by hand); trailing `/` only on an endpoint's
own path (Sentry repeats `{issue.issue}/` in 4 endpoints); one visibility for the whole declaration.
**Precedent — constructible but refused before I/O:** PayPal policy refuses other origins (test:
0 requests at both servers, tests/protocol.rs:728-739); Strava "credential destination changed"
(tests/protocol.rs:363-371); invalid setter values give a Build error before I/O.

## 4. Scope representation
Flattening forms (obs): **F1** parent identity repeated on sibling endpoints (GitHub `repo` 11
endpoints in 4 families: issue, pull, run, artifact; OpenAI `files/{id}` ×2 + batch; Sentry 4);
**F2** context on the client instead of a scope (QuickBooks realm — forced; Stripe
`account_override` on 19 endpoints (38 lines) + `OnAccount` macro; Twilio account SID; Zendesk
subdomain; ClickHouse database/quota); **F3** a scope rebuilt as another client (Distribution
`Repository`, B2 role hosts, Mastodon streaming host); **F4** a scope mirrored/flattened by a facade
(Xero `Tenant`, Kubernetes `NamespaceContext` across 4 group scopes, GitHub `RepoContext`, Shopify
`ShopContext`, Datadog, Elasticsearch, Dropbox (deliberate cursor binding), Airtable); **F5**
single-endpoint identity as arguments (Nager, Frankfurter, World Bank — a scope would add ceremony).
15 of 55 SDKs declare no scope. Reasons given: `send`/`base` client-only; one-segment holes; every
value has a public setter; per-request overrides declared per endpoint; cross-sibling scopes; cursor
binding. Scope-change cost: Adyen (generated client used directly) adds a scope by declaration only
(MAINTENANCE.md:45-49); Kubernetes (facade) adding an API group: declaration +65/−4, facade +20/−8, 2
adapter registrations (checks/maintenance-kubernetes/MAINTENANCE.md:89-118); moving/renaming breaks
callers (AUTHOR_GUIDE:2221-2223); QuickBooks realm, Distribution repository, B2 role host, Stripe
scope-wide override cannot be expressed. Caller exposure (call sites): GitHub `.repo(` 76, Xero
`.tenant(` 62, Stripe overrides 59, Airtable `list_page(s)` 41, Shopify `.shop(` 19.

## 5. Options (only M and R are removal targets; S boundaries must survive, ideally generated)
| Option | Lets SDKs delete | Must preserve | Concept cost | Breakage |
|---|---|---|---|---|
| K keep current | nothing | — | 0 | 0 |
| A `#[fixed]` value (supplied at construction/entry, no setter) | few M; turns the §3 invariant breaks (shop/token, subdomain, credential+state, Salesforce version, Xero tenant) into "unavailable" | rotation via unmarked values | +1 attribute | opt-in; marking removes a public method |
| B runtime base (`base;` → validated `BaseUrl` in `new`, no `with_base_url`; optionally `base` on scopes) | Mastodon S + ~6 M; DigitalOcean R; 8 placeholders; B2 closure | fixtures via loopback constructor; `with_base_url` on literal-base clients; S3-compatible endpoints | +1 statement form, +1 type | opt-in; adopters change `new` |
| C destination-bound credentials (generated; other origin refused at send) | PayPal/Strava checks; hardens public clients without author action | fixture flows that move the base after `new` | hidden rule | high (inf) |
| D author-controlled visibility of controls (`controls { with_base_url: crate; set_tenant: crate; }`) | forwarders in SDKs hiding only to withhold controls (Mastodon ~6-7 M; B2, Graph, Drive, Snowflake could expose scopes/endpoints); matches Xero F2 | public rotation controls | moderate | opt-in |
| E coordinated replacement groups (`replace auth(token, limiter);`) | K8s `with_bearer`, Mastodon/ntfy `with_token`, part of S3 (4-6 M); excludes the Discord/Sentry/Adyen/Snowflake mismatches | QuickBooks single value; S3 rotation | +1 statement | opt-in |
| F getters for non-sensitive values and the base | ~30 copied-value M | no getter for `#[sensitive]`/policy values | low | name collisions (rule exists) |
| G typed credentials (unseal `ToHeaderValue` or a typed `#[sensitive]` recipe) | ~14 token→header constructors | validation at the newtype constructor | 0-1 | per adopter |
| H scope capabilities (`send`/base on scopes, multi-segment holes, scope-wide override, trailing slash on scope paths) | F2/F3 forms; QuickBooks map-of-clients; Stripe 38 lines + macro | — | +1 rule each | none if additive |
Assessment (inf): K defensible (most S/P deliberate and tested) but leaves §3 holes open; A and D aim
at the observed holes and forwarders at the lowest concept cost; B targets the named witness and the
placeholders; C is the only automatic hardening but highest breakage; F removes the largest M
cluster; simple controls untouched by any opt-in option.

## 6. What could be measured automatically
Mechanics: per-SDK probe crates with `compile_fail` doctests for forbidden moves (precedent:
discord api.rs:71-88, b2 upload.rs:167); run-time probes against `examples/testserver` with a second
"evil" server. Per probe: forbidden move available (0/1); rejection stage (compile / construct /
build / send before I/O / never); valid replacements succeed (must stay 100%); requests at the evil
server (must be 0).

| SDK | Forbidden moves | Valid moves that must keep working | Expected today |
|---|---|---|---|
| Mastodon (witness) | host change keeping the token; stream to another instance | `with_token`, discover, first call | compile_fail; send before I/O |
| Xero (witness) | `set_tenant`, `set_policy`, `with_base_url`; other tenant's ids | reopen, `tenant()`, list/get | compile_fail; construct |
| B2 (witness, issued host) | token to a non-issued host; https→http | authorize, upload | contract error |
| Salesforce | `api().clone().with_base_url(evil)`; `set_version(<MINIMUM)` | construct + first call | never rejected |
| Shopify | `admin().clone().set_shop(b)` | construct + first call | never rejected |
| Zendesk | `set_subdomain(other)` | construct + first call | never rejected |
| QuickBooks (contrary) | `new(sandbox_conn)` at the production host; `with_base_url(evil)` | `set_connection(conn_b)` (realm B in path, B's token on the wire) | constructible |
| S3 / K8s / Spotify (rotation, contrary) | — | S3 `with_credentials`, K8s `with_bearer` + `set_authorization`, Spotify `replace_grant` | must pass |
| PayPal / Strava (positive controls) | moved client | — | 0 requests |
| Discord / Sentry / Adyen | credential setter keeping old per-credential state | — | constructible |
| Nager / PokeAPI / Frankfurter (simple controls) | — | `X::new(http)`, no Config, no facade | no added arguments/imports/steps (P9) |

Complete ordinary use: construction + first call from public docs, checked at the local server
(method, path, query, host, sensitive auth header, decode); also whether the wrong constructor
compiles and fails on the wire (`QuickBooks::new` vs `connect`, `DatadogApi::new` vs
`DatadogEvents::new`, `SalesforceApi::new`). Scope-change cost: pre-registered edits (GitHub extract
`pull`/`run`; OpenAI `file(id)`; Kubernetes API group replay; Xero level; Adyen `payment(..).refunds`;
Stripe account as a scope; QuickBooks realm / Distribution repository / B2 role host
expressibility; Frankfurter `pair` without ceremony), scored by authoritative edit sites, missed
propagation and its stage, wire equality of unchanged operations. Static census F1–F4 and M/P/S/R
counts are diagnostics only (A1 §2.3).

Caveats: classification and granularity are the surveyor's; regex counts approximate; every
"Expect" is unverified.
