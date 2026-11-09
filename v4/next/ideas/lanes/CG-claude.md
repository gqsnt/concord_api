# Families C and G: construction, facades and naming (lane: claude)

Repository read: `/home/canarit/projects/Concord_Next/work/wt/rv-p1` at `ba10189b` (detached HEAD, read only).
Inputs read: `CHARTER.md`, `IDEAS.md` (families C and G), `v4/next/inputs/CONSTRUCTION_PROBLEM.md`,
`work/glue-dev/GLUE.draft.md`, `docs/CHEATSHEET.md`, `docs/AUTHOR_GUIDE.md` (§1-5.14, 7-11),
`crates/concord-macros/src/emit.rs` (client/scope emission), `crates/concord-core/src/{header.rs,
__private.rs, error.rs, prepared.rs}`, the commit that removed `controls` (`2f34ce5a`) and the H06
measurement record on `origin/archive/desirability-loop-1`. `work/inventory/FRICTION.md` does not
exist yet, so the witnesses' own `FRICTION.md` files were read instead (Mastodon, Datadog, PayPal,
QuickBooks, Xero, DigitalOcean, Distribution Registry, OpenAI, Elasticsearch, Zendesk).
SDK sources read: the 14 witnesses, Amazon S3, Kubernetes, ClickHouse, the three simple controls,
and the construction or forwarding runs of Microsoft Graph, Snowflake, Mailgun, Twilio, HubSpot,
Linear, Kraken, B2, Dropbox, Strava, World Bank and Notion.

How the numbers were made:
- **Observed:** the quoted code and every `file:line` range. Line counts are non-blank lines of
  those ranges (`sed -n a,bp | grep -cv '^\s*$'`).
- **Observed:** the forwarder table in G comes from `classify.py` runs (`glue:forwarding`, 41 runs,
  829 lines). The cause of each run is my reading of it.
- **Estimated, not built:** "SDK code that disappears" is my judgement of which lines a design makes
  unnecessary. Nothing here was compiled. The estimates overlap where a wrapper needs several
  designs to go away, so the per-design totals must not be added up.

Names used below: **FM1-FM4** are the forbidden moves of `CONSTRUCTION_PROBLEM.md`. "H06c rows" are
the 11 rows of its audit table.

---

## Summary

| Design | Provider fact it states | Mechanism | Est. SDK lines removed | Also removes glue in |
|---|---|---|---|---|
| **C-A** Fixed unless `mut` | "this value is replaced during the client's life" (a token rotates) | required client/scope values get no `set_x` unless declared `mut` | ≈0-20 directly; it lets the wrapper deletions below happen without reopening FM1/FM2/FM4 | H (fewer setter docs) |
| **C-B** The base is a value | "the host is the caller's / the site's / the environment's" | `base instance;` with `instance: T`, `T: concord::Origin`; no `with_base_url` | ≈350-400 (10 SDKs) | G (wrappers, aliases), H (no placeholder hosts in docs) |
| **C-C** Secrets stay on declared origins *(smaller than you think)* | none (a consequence of `#[sensitive]`) | on a client holding a secret, `with_base_url` accepts only a local test origin | ≈125 (7 SDKs), +≈90 with the transport builder | D (a tested destination guarantee), J |
| **C-D** Credentials are typed values | "the credential is a Bearer / Basic / `ApiKey` / bare key" | Concord-owned `concord::auth::{Bearer, Basic, Authorization, Key}` in the `ToHeaderValue` set; optional `header(..)` writers | ≈300-400, of which ≈180 are SDK token newtypes with hand-written `Debug` | D (redaction), G (`connect` wrappers) |
| **C-E** The send policy reads declared values | "SigV4 signs with the endpoint's region, the credentials and the time" | `send SigV4 { endpoint, credentials, clock };` builds the policy from client values for each send | ≈200 (S3, PayPal, Shopify) | E (the policy reads facts instead of copying them), A |
| **G-A** Read accessors *(smaller than you think)* | none (typed access to declared facts) | `client.version()`, `scope.shop()`, `client.http()`, `client.base_url()`; secrets' getters are `pub(crate)` | ≈70 directly; it enables every wrapper deletion above | C, E, F (policies and pagers read values) |
| **G-B** Build errors name the value *(smaller than you think)* | none | `concord::Error::invalid_value()` → `{ what, rule }` | ≈50 (OpenAI, Drive, HubSpot, Linear, Twilio) | A (SDK error variants such as `EmptySid`) |
| **G-C** Recognize returned URLs | the URL shape the declaration already states | `Endpoint::from_url(&client, url)`: the inverse of `prepare()` | ≈350 (Salesforce ≈250, PokeAPI ≈95, Registry ≈30) | F (link paging), D (URLs never shown) |
| **G-D** Nest by context, not by URL | none (no language change) | a pathless context scope whose children read `{namespace.namespace}` | ≈50 (Kubernetes) | none |

**Which invariant breaks each design closes.** ✓ = closed. Blank = not addressed. Text = partly closed.

| Row (CONSTRUCTION_PROBLEM.md) | C-A | C-B | C-C | C-D | C-E |
|---|---|---|---|---|---|
| FM1 Shopify `set_shop` | ✓ | | | | |
| FM2 Zendesk `set_subdomain` / `with_base_url` | ✓ (setter) | | ✓ (base) | | |
| FM3 Salesforce `with_base_url(evil)` | | ✓ | ✓ if the placeholder base becomes `base instance;` | | |
| FM4 Salesforce `set_version(< MINIMUM)` | ✓ (needs `ApiVersion` to refuse old versions to cover `new`) | | | | |
| Adyen `set_api_key`, `set_policy` | ✓ | | | | |
| Datadog `new`, `set_api_key`, `set_application_key` | ✓ (setters) | | | ✓ (`new` takes validated keys) | |
| DigitalOcean `with_base_url` | | | ✓ | | |
| Discord `set_token`, `set_limiter` | ✓ | | | | |
| Mastodon (generated client kept private) | | ✓ | ✓ with C-B (its own `with_base_url(instance)` would be refused) | | |
| QuickBooks `with_base_url` | | ✓ (`base connection;`) | ✓ with C-B (`connect` moves to the sandbox host, which is not local) | | |
| Salesforce `new`, `with_base_url`, `set_version` | ✓ | ✓ | | | |
| Sentry `set_authorization`, `set_policy`, `with_base_url` | ✓ | | ✓ | | |
| Zendesk | ✓ | | ✓ | | |
| PayPal (generated `new` cannot be built, FRICTION §7) | | | | | ✓ |
| USGS (`new`, `with_base_url`: "the capsule runs only against local fixtures") | out of scope: an SDK test rule on an API without secrets, not a provider fact | | | | |

**How they compose.** **C-A + C-C + C-B** close every row except Datadog `new` (which needs
**C-D**) and USGS (out of scope). C-C cannot stand alone: a client that holds a secret and moves
today to a non-local run-time host must declare that host as a value (C-B). So C-C's breaking list
is C-B's adoption list. **G-A** is what makes deleting a wrapper possible: without it, every
wrapper keeps a copy of the values it gave to `new`.

---

# Family C: what is fixed at construction and what may change

## Seeds: kept, changed, rejected

- **Seed 1 (fixed by default, `mut` for what changes):** kept as C-A. Two changes: the rule covers
  only *required* values (optional and initialized values are settings with a default, and keep
  their setters), and `#[rotates]` is dropped in favour of the Rust keyword.
- **Seed 2 (base selection as a fact):** kept as C-B (`base value;` rather than a host template;
  the template is discussed in C-B). The test-only constructor (`#[cfg(test)]` or a `fixture`
  feature) is **rejected**. Integration tests live in `tests/`, which is another crate where
  `cfg(test)` is off. A Cargo feature is unified across the dependency graph, so one dependent that
  enables `fixture` enables it for every user of the SDK. C-C's local-origin rule needs neither.
- **Seed 3 (grouped values, `credential (realm, token)`):** **rejected.** No witness needs it. A
  pair that rotates together is already one SDK type (QuickBooks `Connection`: realm, token source
  and environment). A pair that must never be split is fixed under C-A (Discord token and limiter,
  Datadog keys, Zendesk subdomain and token, where only the token rotates). With C-B and C-E, one
  value can be the base, the path segment and the policy at once (QuickBooks below), so there is
  nothing left to group.
- **Seed 4 (constructors take the SDK's validated types):** kept as C-D. The obstacle is concrete:
  `ToHeaderValue` is sealed (`crates/concord-core/src/header.rs:36`), so every SDK declares a raw
  `HeaderValue` and wraps the constructor.
- **H06 arm A (`#[fixed]`):** not reused. It is the default spelled as an exception, and it left
  FM3 open (H06 measurements: "FM3 … available; 1 request at evil").

## C-A: Fixed unless declared `mut`

**Fact it expresses:** "this client value is replaced during the client's life" (an OAuth token
expires, a connection is re-authorized). Every other *required* client or scope value is the
identity of the handle and stays as it was given.

**Declaration (author writes):** Zendesk, `examples/zendesk-support-sdk/src/lib.rs:43-51`, before:
```rust
pub client Zendesk(
    /// The account subdomain (`acme` for `acme.zendesk.com`).
    subdomain: String,
    /// `Bearer {access_token}`; build it with [`Zendesk::connect`] or
    /// `concord_auth::bearer(token)`.
    #[sensitive] header("authorization") authorization: HeaderValue,
    /// How generated `send()` treats reads and writes.
    policy: SendPolicy,
) {
```
after:
```rust
pub client Zendesk(
    /// The account subdomain (`acme` for `acme.zendesk.com`).
    subdomain: String,                                            // no `set_subdomain` (FM2)
    /// `Bearer {access_token}`, rotated by the application.
    #[sensitive] header("authorization") mut authorization: HeaderValue,  // `set_authorization`
    /// How generated `send()` treats reads and writes.
    policy: SendPolicy = SendPolicy::default(),                   // a setting: `set_policy`
) {
```
Shopify scope, `examples/shopify-sdk/src/api.rs:38-41`, before → after:
```rust
scope shop(shop: ShopDomain, #[sensitive] header(X_SHOPIFY_ACCESS_TOKEN) access_token: String) {
// ↓
scope shop(shop: ShopDomain, #[sensitive] header(X_SHOPIFY_ACCESS_TOKEN) mut access_token: String) {
```
Discord (`examples/discord-rest-sdk/src/api.rs:36-44`) and Salesforce `version`
(`examples/salesforce-sdk/src/api.rs:39`) need **no edit**. Their required values simply lose
their setters, so "one limiter per bot token" and "the version `OrgContext::new` checked" hold
without any word in the declaration.

**Call site (caller writes):**
```rust
// before: compiles; shop A's token goes to shop B (FM1)
let moved = shop.admin().clone().set_shop(shop_b);
// after: error[E0599]: no method named `set_shop`. Enter the scope again:
let b = admin.shop(shop_b, token_b);
// rotation in the same shop, unchanged:
let shop = shop.set_access_token(fresh);
// Discord before: `discord.set_token(t2)` kept the old token's limiter. After: no setter, so
let d2 = Discord::new(http, DiscordConfig { token: t2, limiter: RateLimiter::new() });
```

**SDK code that disappears:** almost nothing directly: ≈0-20 lines of doc sentences that warn
about the setters (for example `xero-accounting-sdk/src/lib.rs:142`: "carrying no publicly mutable
credential or tenant setters"). Its value is indirect. It removes the *reason* for the wrappers
deleted under C-B, C-D, C-E and G-A. Without it, deleting Shopify `context.rs:28-139` (99 lines),
Salesforce `context.rs:36-133` (87) or Datadog `client.rs:41-82` (41) would reopen FM1, FM4 and the
Datadog row. Generated rustdoc also loses one setter per required client or scope value (family H).

**Also removes glue in:** H (one setter doc fewer per fixed value). It is also the precondition that lets C-B, C-D, C-E and G-A delete wrappers in G.

**Hardest counterexample:** QuickBooks, coordinated realm and credential replacement. It is one
value, so `mut connection: Connection` gives `set_connection`, which replaces realm, token source
and policy together (and the host too, with C-B). No grouping syntax is needed. Credential rotation
works the same way: S3 `mut credentials` (with C-E), Kubernetes' optional `authorization?` (already
settable).

**Simple control (Nager):** unchanged. Nager, PokeAPI and Frankfurter have no client values.

**Charter check:**
- C1: `mut` states a positive provider fact (the value changes). It is not a negation of generated surface.
- C2: *when* to rotate stays in Rust. The declaration says only *that* it may.
- C3: the simple controls have no client values.
- C4: one deterministic rule, visible on the parameter: "a required client or scope value has no setter unless `mut`".
- C5: plain E0599 names the generated type. Optional improvement: generate a `#[doc(hidden)] set_shop<V: concord::Fixed>` whose `on_unimplemented` message says "`shop` is fixed when the scope is entered: call `admin.shop(..)` again".
- C9: trybuild cases for FM1, FM2 (setter) and FM4.

**New concepts:** the `mut` marker on a required client or scope value.

**Breaking changes:**
- `set_x` disappears for required client and scope values that are not `mut`.
- Observed call sites:
  - Zendesk `set_policy` (tests/support/mod.rs:51, canary.rs:67, first_use.rs:320): fix by making the policy initialized.
  - Cloudflare `set_policy` (6 test sites, README:192): same fix.
  - QuickBooks `set_connection` (tests/environment.rs:57,178, policy.rs:212): `mut`.
  - S3 `set_signer` (client.rs:256): `mut`, or C-E.
  - NASA `set_api_key` (tests/workflows.rs:61), Anthropic `set_api_key` (tests/first_use.rs:210): `mut`, or build a new client.

**Why it might be wrong:** "Required" stands in for "identity": an initialized value can carry
identity (C-B's `server: BaseUrl = …`) and stays settable. A smaller variant with no keyword
(rotation is always "build a new client") would close the same FMs, but the fact would no longer be
in the declaration, and the charter lists "values fixed at construction vs changeable" as a fact to
declare.

## C-B: The base is a declared value (`base instance;`)

**Fact it expresses:** "requests go to the host the caller names (a self-hosted instance, an org's
instance URL, a deployment) or to the host of a site or environment" (Datadog site, QuickBooks or
PayPal environment). Today this is a placeholder base plus `with_base_url` (`mastodon.invalid`,
`instance.salesforce.invalid`, `account.snowflake.invalid`, `registry.invalid`) or an arbitrary
default (Datadog US1, PayPal sandbox).

**Declaration (author writes):** Salesforce, `examples/salesforce-sdk/src/api.rs:39-42`, before:
```rust
pub client SalesforceApi(#[sensitive] token: String, version: ApiVersion) {
    base "https://instance.salesforce.invalid";
    path "services/data/v{version}";
    headers { AUTHORIZATION = "Bearer {token}"; }
```
after:
```rust
pub client SalesforceApi(instance: InstanceUrl, #[sensitive] token: String, version: ApiVersion) {
    base instance;                       // `InstanceUrl: concord::Origin`; fixed (C-A)
    path "services/data/v{version}";
    headers { AUTHORIZATION = "Bearer {token}"; }
```
Datadog, `examples/datadog-events-sdk/src/api.rs:26-32`, before:
```rust
pub client DatadogApi(
    #[sensitive] header(DD_API_KEY) api_key: String,
    #[sensitive] header(DD_APPLICATION_KEY) application_key: String,
) {
    base "https://api.datadoghq.com";
```
after:
```rust
pub client Datadog(
    /// The site the organization, and therefore both keys, belong to.
    site: Site,
    #[sensitive] header(DD_API_KEY) api_key: String,
    #[sensitive] header(DD_APPLICATION_KEY) application_key: String,
) {
    base site;
```
QuickBooks, `examples/quickbooks-online-sdk/src/lib.rs:67-80`, one value used three ways:
```rust
pub client QuickBooks(mut connection: Connection) {
    base connection;                  // its environment's host (`impl concord::Origin for Connection`)
    path "v3/company/{connection}";   // its realm (`Display`), as today
    ...
    send connection -> Result<_, Error>;   // its token, as today
```
The SDK implements one small trait. It replaces Datadog's `api_base_url()` + `.expect(..)`
(`client.rs:78-80`), and QuickBooks' and PayPal's base strings:
```rust
impl concord::Origin for Site {
    fn base_url(&self) -> concord::BaseUrl {
        match self {
            Site::Us1 => concord::base!("https://api.datadoghq.com"), // checked like `base ".."`
            Site::Eu => concord::base!("https://api.datadoghq.eu"),
            /* … */
        }
    }
}
```
The Concord side:
- `concord::BaseUrl` is today's `#[doc(hidden)] __private::Base` made public (`parse`, `url()`).
- `concord::base!` is the macro-time check that `base ".."` already runs.
- `Origin` is implemented for `BaseUrl`.
- A client whose base is a value has **no `with_base_url`**. A run-time origin advertised by the
  first one (the Mastodon streaming host, a B2 upload URL, a Registry token realm) is a second
  client with its own `base host;`.

**Call site (caller writes):**
```rust
// before (salesforce lib.rs:32-37, 44)
let ctx = OrgContext::new(http, InstanceUrl::parse("https://acme.my.salesforce.com")?,
                          ApiVersion::PINNED, AccessToken::new(token)?)?;
let page = ctx.api().query().accounts(soql).send().await;
ctx.api().clone().with_base_url(evil)?;          // FM3: compiles, token sent to `evil`
// after
let api = SalesforceApi::new(http, SalesforceApiConfig {
    instance: InstanceUrl::parse("https://acme.my.salesforce.com")?,
    token, version: ApiVersion::PINNED,           // `ApiVersion` refuses < MINIMUM itself
});
let page = api.query().accounts(soql).send().await;
api.with_base_url(evil);                          // error[E0599]: no method `with_base_url`
// Datadog before: DatadogEvents::new(http, Site::Eu, Credentials::new(k, a)?)
let dd = Datadog::new(http, DatadogConfig { site: Site::Eu, api_key: k, application_key: a });
```

**SDK code that disappears:** ≈350-400 lines (counts overlap with G-A and C-D):
- Mastodon ≈120:
  - `lib.rs:119-150` (30, wrapper and `new` with `unreachable!`), `152-176` (21, `instance_url` + four pure forwarders), `97-102` (6, `ApiX as XRequest` aliases), `242-247` (6, `Debug`).
  - `model.rs:20-67` (45 → ≈10): `InstanceUrl::parse`/`check_origin` restate `Base::parse` (Mastodon FRICTION §1).
  - `lib.rs:192-239` (45 → ≈12): the three stream methods become navigation on the second client. The "advertised by this instance" check stays.
- Salesforce ≈77: `context.rs:36-133` (87). The `ContextError::Base` variant goes, and the version check moves into `ApiVersion`.
- Datadog ≈60: `client.rs:41-82` and `97-124` (struct, `new`, `with_base_url`, `site`, `api`, `events`). The workflows stay as `impl Datadog`.
- QuickBooks ≈35:
  - `lib.rs:157-168` (12, `connect`).
  - `policy.rs:35-37` and `47-62` (19, `select_environment`: the policy rewrites the host because `set_connection` cannot).
  - The doc sentences at `lib.rs:10-12, 64-66, 69-71`.
- Elasticsearch ≈28: `lib.rs:151-180` (struct, `new`, `api`).
- Distribution Registry ≈25: `client.rs:74-95` (URL validation and `with_base_url`).
- PayPal ≈25:
  - `lib.rs:83-91` (`base_url`).
  - `policy.rs:208-210` and `Error::Destination` (error.rs:304, 358), the check that the request still targets the session's origin (FRICTION §6).
- Kubernetes, ClickHouse, Mailgun, Snowflake: the `with_base_url` or region-to-base glue, ≈10 each.

**Also removes glue in:** G (Mastodon's pure forwarders and `ApiX as XRequest` aliases, Elasticsearch's and Datadog's `api()` escape accessors) and H (generated docs stop printing `mastodon.invalid` and "the client's `with_base_url` replaces it").

**Hardest counterexample:** Mastodon's streaming host. It is a second origin, discovered at run
time and trusted only when advertised by *this* instance. The declaration gets a second client
(`client MastodonStreaming(host: StreamingHost, …) { base host; … }`) and the SDK keeps an
≈8-line method that checks `host.instance() == api.instance()` (G-A) before building it.
Fixtures: an SDK type decides whether it accepts a loopback origin (Mastodon's `InstanceUrl` does).
Enum-typed values (Datadog `Site`, QuickBooks `Environment`) need C-C's local override.

**Simple control (Nager):** unchanged. `base "https://nagerholidays.com";` stays a literal, with
the same `with_base_url`.

**Charter check:**
- C1: "the host is the instance the caller names" is a provider fact (self-hosted products, regions, environments).
- C2: no behavior. The value's type maps itself to a URL.
- C3: literal bases unchanged.
- C4: `base x;` names the value, visible in the parameter list. The URL itself is in Rust (see "Why it might be wrong").
- C5: a type without `Origin` is reported at `base x;` with `on_unimplemented` ("`Site` is not an origin: implement `concord::Origin`").
- C7: `prepare()`, `into_parts()` and `follow` are unchanged.
- C8: the base is computed from the value at `prepare()`, deterministically.
- C9: wire tests per value (Datadog already has `sites_match_the_documented_table`). Compile-fail: `with_base_url` absent.

**New concepts:** `base <value>;`, `concord::Origin`, public `concord::BaseUrl`, `concord::base!`.

**Breaking changes:** SDKs that adopt it lose `with_base_url` on that client (an opt-in per
declaration). Generated docs print `GET {instance}/…` instead of a placeholder host.

**Why it might be wrong:** The URLs move into an SDK `impl` (`base site;` shows no host). A host
template (`base "https://api.{site}"`, as Datadog's FRICTION §1 suggests) keeps them visible but
cannot express Mastodon, Salesforce or S3-compatible origins. Second origins (Mastodon streaming,
B2 uploads) cost extra client declarations, unless scopes may also rebase, which raises the
question of which ancestor paths still apply.

## C-C: Secrets stay on declared origins (no new syntax). *Smaller than you think*

**Fact it expresses:** none new. Concord already knows every secret. A client whose
declaration holds a secret at client or scope level (a `#[sensitive]` value, a value under a
well-known secret name, or a `send` value) sends it only to its declared base, or to a local test
origin. This is the same reasoning the charter uses for logging: Concord is the only layer that
knows every secret, so it is the layer that can keep them home.

**Declaration (author writes):** nothing changes. DigitalOcean, `examples/digitalocean-sdk/src/lib.rs:24-25`:
```rust
pub client DigitalOcean(#[sensitive] header(AUTHORIZATION) authorization: reqwest::header::HeaderValue) {
    base "https://api.digitalocean.com";
```
The generated `with_base_url` of such a client accepts only:
- loopback IPs (`127.0.0.0/8`, `::1`);
- `localhost` and `*.localhost`;
- `*.test` (RFC 6761: never resolvable on the public Internet).

Anything else is `ErrorKind::Build`, with the message: "this client holds a secret
(`authorization`); its requests go to `https://api.digitalocean.com` or a local test origin.
Declare a host the caller chooses with `base <value>;`."

**Call site (caller writes):**
```rust
// before (digitalocean lib.rs:73-96, 28 lines of loopback checks)
let api = DigitalOcean::for_fixture(TOKEN, &server.url())?;
DigitalOcean::connect(TOKEN)?.with_base_url("https://evil.example")?;   // compiles, sends the token
// after
let api = DigitalOcean::connect(TOKEN)?.with_base_url(server.url())?;  // loopback: accepted
DigitalOcean::connect(TOKEN)?.with_base_url("https://evil.example");   // Err(Build), nothing sent
```

**SDK code that disappears:** ≈125 lines of hand-written "HTTPS for production, loopback for
tests" rules, seven copies of one rule:
- DigitalOcean `lib.rs:69-96` (28, `for_fixture`);
- Graph `client.rs:94-127` (32 → ≈6, `for_loopback`; the transport choices stay);
- Drive `client.rs:88-105` (18);
- Xero `lib.rs:123-139` (17, `validate_origin`);
- Mailgun `lib.rs:107-118` (11, `local`);
- Snowflake `client.rs:91-105` (15);
- PayPal `policy.rs:208-210` + `Error::Destination` (≈8, with C-B).

Optional companion: `concord::transport()`, a `reqwest::ClientBuilder` with redirects off, no
retry and no referer. It replaces the copied `http_client()` functions (Zendesk `lib.rs:134-147`
14, QuickBooks `170-183` 14, PayPal `145-159` 14, Datadog `client.rs:84-95` 12, ClickHouse
`client.rs:176-187` 11, DigitalOcean `lib.rs:99-108` 10, S3 `client.rs:27-39` 13: ≈90). The
caller's own `reqwest::Client` stays accepted (C7).

**Also removes glue in:** D and J (a tested guarantee about *where* secrets go, next to "secrets never print").

**Hardest counterexample:** proxies and gateways. A forward proxy configured on the
`reqwest::Client` keeps the origin and is unaffected. A gateway that *replaces* the host (Datadog
`with_base_url` "for a proxy or a local stand-in", `client.rs:97`) is refused unless the SDK
declares the host as the caller's (C-B). An application can still leave through
`prepare()?.into_parts()` and execute anywhere (C7, explicit). Tests that map a *real* domain with
`ClientBuilder::resolve` (Shopify `tests/wire.rs:33` `http://myshopify.com:{port}`) must move to
`myshopify.test`. Zendesk (`zendesk.test`, `acme.localhost`) and S3 (`s3.test`) already comply.

**Simple control (Nager):** unchanged. Nager, PokeAPI and Frankfurter hold no secret, so
`with_base_url` accepts any http/https origin as today (their docs promise "another deployment").

**Charter check:**
- C1: no new words.
- C2: a refusal, not a behavior.
- C3: no-secret clients unchanged.
- C4: keyed on `#[sensitive]`/`send`, which are visible; the well-known-name trigger is less visible (see below).
- C5: the `Result` of `with_base_url` at the caller's line names the value.
- C7: `into_parts()` is still the way out.
- C8: deterministic.
- C9: canary test: a secret-bearing client pointed at a non-local origin sends nothing; a local one receives the secret.

**New concepts:** none in the language. One documented rule; optionally `concord::transport()`.

**Breaking changes:**
- `with_base_url(non-local)` fails on every client holding a secret.
- Observed users that must adopt C-B first:
  - Mastodon, Salesforce, Elasticsearch, ClickHouse, Kubernetes, Datadog, PayPal (live), QuickBooks (environment), Snowflake, Mailgun (region), S3 (`with_endpoint`);
  - Distribution Registry (three clients), B2 (upload URL), HubSpot (`crm_base`).

**Why it might be wrong:** It cannot see redirects: reqwest forwards a custom credential header
across origins (author guide §3), so the guarantee is "Concord never *targets* another origin", not
"the secret never arrives". Adding a doc-only `#[sensitive]` to a non-credential now restricts
`with_base_url`, a behavior change triggered by a redaction marker.

## C-D: Credentials are typed values

**Fact it expresses:** the provider's authentication scheme. `Authorization: Bearer <token>`,
`Basic`, `ApiKey <key>` and a bare key in a vendor header are facts the declaration can name by
type. Today `ToHeaderValue` is sealed (`header.rs:36`), so the declared type is a raw
`HeaderValue`. The generated `Config` then takes a raw header, and every SDK wraps `new`.

**Declaration (author writes):** Twilio, `examples/twilio-sdk/src/lib.rs:23-29`, before:
```rust
pub client TwilioApi(
    #[sensitive] authorization: HeaderValue,
    #[sensitive] account_sid: String,
) {
    base "https://api.twilio.com";
    path "2010-04-01/Accounts/{account_sid}";
    headers { AUTHORIZATION = authorization; }
```
after:
```rust
pub client Twilio(
    #[sensitive] header(AUTHORIZATION) auth: concord::auth::Basic,
    #[sensitive] account_sid: String,
) {
    base "https://api.twilio.com";
    path "2010-04-01/Accounts/{account_sid}";
```
Zendesk (`lib.rs:48`):
`#[sensitive] header("authorization") mut authorization: concord::auth::Bearer`.

Elasticsearch (`lib.rs:62`):
`#[sensitive] header(AUTHORIZATION) api_key: concord::auth::Authorization`.

Datadog (`api.rs:28-30`): two `concord::auth::Key` values, each validated where it is built.

The second part, optional, is for a credential that writes several headers. ClickHouse,
`examples/clickhouse-sdk/src/wire.rs:24-30`, before:
```rust
#[sensitive] header("authorization") authorization?: HeaderValue,
#[sensitive] header("x-clickhouse-user") user?: String,
#[sensitive] header("x-clickhouse-key") key?: String,
```
after (`header(..)` mirrors today's `query(..)`; `Credentials: concord::HeaderPairs` lists its
possible names so composition can be checked):
```rust
/// None, HTTP Basic, or the `X-ClickHouse-User`/`X-ClickHouse-Key` pair.
#[sensitive] header(..) credentials: Credentials = Credentials::none(),
```

**Call site (caller writes):**
```rust
// before (twilio lib.rs:77-87; zendesk lib.rs:117-131)
let twilio = Twilio::new(http, sid, BasicAuth::new(user, pass)?);
let zd = Zendesk::connect(http, "acme", &token)?;
// after
let twilio = Twilio::new(http, TwilioConfig { auth: Basic::new(user, pass)?, account_sid: sid });
let zd = Zendesk::new(http, ZendeskConfig { subdomain: "acme".into(), authorization: Bearer::new(&token)? });
```

**SDK code that disappears:** ≈300-400 lines.
- Constructors that only convert a credential:
  - Zendesk `lib.rs:108-132` (25), DigitalOcean `lib.rs:56-67` (12), OpenAI `lib.rs:325-345` (21), Twilio `lib.rs:71-87` (≈15), Elasticsearch `lib.rs:162-175` (≈14), Mastodon `with_token` `lib.rs:142-150` (9), Kubernetes `with_bearer` `context.rs:51-61` (11);
  - ClickHouse `client.rs:28-89` (57: the `Credentials`/`Auth` enum and its `Debug`) + `163-174` (12, the mapping in `new`);
  - Datadog `credentials.rs:27-82` (53) + `client.rs:69-82` (14).
- SDK token newtypes (`struct X(HeaderValue)` + `new` + `into_header` + redacted `Debug`, ≈25-35 lines each). These are family D glue:
  - Elasticsearch `ApiKey` (model.rs:100), Kubernetes `BearerToken` (ids.rs:223), Mastodon `AccessToken` (model.rs:108);
  - Salesforce `AccessToken` (ids.rs:237), Shopify `AccessToken` (scalars.rs:161), Twilio `BasicAuth` (auth.rs:12).
  - ≈180 lines in all.

**Also removes glue in:** D (six hand-written redacting `Debug` impls of token newtypes, plus Datadog's and ClickHouse's `Credentials` `Debug`) and G (`connect`/`new` wrappers).

**Hardest counterexample:** ClickHouse. Its credential is one of three header sets, so it needs the
`header(..)` writer. Header-name conflicts are then checked when the request is built (`Build`
error, as for native header constants today), with `HeaderPairs::NAMES` letting
`validate_header_names()` see them. Datadog wants per-key error variants
(`AuthError::MissingApiKey`); `Key::new` errors are generic, so the SDK maps them once.

**Simple control (Nager):** unchanged. No credentials.

**Charter check:**
- C1: the scheme is a provider fact, named by a type.
- C2: no refresh. The types only validate and format.
- C3: unchanged.
- C4: visible in the parameter's type.
- C5: the existing `on_unimplemented` of `ToHeaderValue` lists the accepted types.
- C7: `HeaderValue` is still accepted.
- C9: canary: `Bearer`'s `Debug`/`Display` never show the token. A `header(..)` value's headers are all withheld from `Prepared`'s `Debug`.

**New concepts:** `concord::auth::{Bearer, Basic, Authorization, Key}` (in the sealed set: no
`Display`, flagged sensitive, redacted `Debug`); optionally `header(..)` and `concord::HeaderPairs`.

**Breaking changes:** none to existing declarations (opt-in types). The `concord_auth::bearer/
basic/authorization` functions become the types' constructors (the old functions can stay
deprecated).

**Why it might be wrong:** SDKs whose token carries provider rules (Salesforce, Shopify) may keep a
newtype anyway, and would need `ToHeaderValue` unsealed after all. Unsealing it lets a type with
`Display` be placed both as a header and as text, which is the classification split `header.rs:26-29`
was written to prevent. The client-level `header(..)` writer moves name checks from compile time to
build time.

## C-E: The send policy reads declared values (`send SigV4 { endpoint, credentials, clock };`)

**Fact it expresses:** which declared values the provider's request signing or authentication uses
("SigV4 signs with the endpoint's region and the credentials at the current time"). Today a
stateful policy *is* one client value (`send signer;`). That value bundles state the declaration
could hold, and so needs hand-written rebuilding to rotate one part, plus a hand-written builder
when the bundle has no public constructor (PayPal FRICTION §7).

**Declaration (author writes):** S3, `examples/amazon-s3-sdk/src/api.rs:33-38`, before:
```rust
pub client S3Api(
    /// The SigV4 context and send policy of every operation.
    signer: Signer,
) {
    base "https://s3.us-east-1.amazonaws.com";
    send signer;
```
after:
```rust
pub client S3Api(
    /// AWS regional endpoint or an S3-compatible server, with its signing Region.
    endpoint: S3Endpoint,
    /// Rotated or refreshed by the application.
    #[sensitive] mut credentials: Credentials,
    /// The signing time source.
    clock: Clock = Clock::system(),
) {
    base endpoint;                                         // C-B
    send SigV4 { endpoint, credentials, clock };           // a struct literal, one per send
```
The rule: `send P { a, b };` evaluates `P { a: a.clone(), b: b.clone() }` from the named client
values (Rust field-init shorthand) and calls `SendWith<T>` on it, as `send value;` does today. The
named values count as used. Each keeps its *own* sensitivity, so `endpoint` is printed and
`credentials` withheld. Today every placement of a policy value is withheld, which is the
QuickBooks FRICTION complaint about the realm segment.

PayPal, `examples/paypal-sdk/src/lib.rs:63-70`, before `pub client PayPal(session: Session) {
base "https://api-m.sandbox.paypal.com"; … send session -> Result<_, Error>;` plus a 92-line
`Builder`. After:
```rust
pub client PayPal(
    environment: Environment,                              // C-B: sandbox or live
    #[sensitive] credentials: Credentials,
    tokens: TokenCache = TokenCache::default(),            // shared by clones
    reads: concord_retry::Policy = policy::default_reads(),
    writes: concord_retry::Policy = policy::default_writes(),
    refresh_margin: Duration = Duration::from_secs(60),
) {
    base environment;
    send Session { environment, credentials, tokens, reads, writes, refresh_margin } -> Result<_, Error>;
```
Shopify, `examples/shopify-sdk/src/api.rs:28-34`: `send ServedVersion { version } -> Result<_,
RequestError>;`. The served-version check (`ShopContext::send` and `check_served_version`,
`context.rs:153-184`) moves into the policy and reads the same `version` the path uses, so the
second copy kept in two context structs goes.

**Call site (caller writes):**
```rust
// before (s3 client.rs:134-142, 232-241)
let s3 = S3Client::new(http, Region::new("eu-west-3")?, credentials)?;
let s3 = s3.with_credentials(rotated);
// after
let s3 = S3Api::new(http, S3ApiConfig { endpoint: S3Endpoint::aws(Region::new("eu-west-3")?), credentials });
let s3 = s3.set_credentials(rotated);
// PayPal before: PayPal::builder(http, credentials).base_url(LIVE).reads(p).build()?
let paypal = PayPal::new(http, PayPalConfig { environment: Environment::Live, credentials }).set_reads(p);
```

**SDK code that disappears:** ≈200 lines.
- S3 ≈85:
  - `client.rs:41-46` (6, `Context`);
  - `157-276` (112 → ≈25 remain: the signed-`prepare`/`sign` escape and the endpoint's domain-host rule move into `S3Endpoint`);
  - the gone parts are `new`, `with_endpoint`/`at`, `with_credentials`/`with_clock`/`with_context`, the `region`/`endpoint` accessors and the `bucket` forwarder.
- PayPal ≈90: `lib.rs:37-142` (92 → `sandbox()` ≈5 remains) and the bundling in `Session::new` (`policy.rs:83-110`).
- Shopify ≈60: `context.rs:28-139` (99), minus the code that moves into the policy (with G-A).

**Also removes glue in:** E (the policy reads the declared facts instead of holding a second copy) and A/B (Shopify's served-version error moves from a wrapper into the policy).

**Hardest counterexample:** PayPal's token cache. It must be shared by clones and bound to one
credential and environment. It becomes an initialized value, so it is shared through `Arc` clones
and cannot be separated from its fixed credentials (C-A). The cost is a public `set_tokens` that
exposes an implementation detail. QuickBooks needs nothing: its policy is already the single
`connection` value.

**Simple control (Nager):** unchanged. `send KeepHead;` is still `KeepHead::default()`.

**Charter check:**
- C1: the named values are what the provider's scheme uses (SigV4 region and credentials, OAuth client credentials).
- C2: "behavior in Rust, with typed access to the facts", literally.
- C3: unchanged.
- C4: a Rust struct literal, visible.
- C5: a missing or extra field is rustc's E0063/E0560 at the `send` statement's span.
- C8: the policy is named on the client; nothing hidden.
- C9: test that a rotated credential is used by the next `send()`.

**New concepts:** the `send P { values };` form.

**Breaking changes:** none (an added form). SDKs that adopt it change their policy type's
constructor.

**Why it might be wrong:** The policy struct's field names become declaration words, so renaming
a field is a declaration edit. It invites moving behavior state (caches, clocks) into client
values, each of which then gets a public setter: generated surface that describes Concord's
plumbing more than the provider.

---

# Family G: facades and naming

## Forwarders by cause (the method IDEAS asks for)

All 41 `glue:forwarding` runs (829 lines) from `classify.py` at `ba10189b`, each assigned to the
dominant cause it shows when read. Mixed runs are split in the notes.

| Cause | Runs (non-blank lines) | Lines | Who removes it |
|---|---|---|---|
| **Construction** (the generated client cannot be handed out, so a wrapper forwards) | Mastodon `lib.rs:158-239` (68); Datadog `client.rs:127-146` (17); Elasticsearch `lib.rs:293-299` (7); QuickBooks `policy.rs:32-44` (12, the host rewrite); B2 `upload.rs:747-765` (19, a run-time upload origin) | 123 | C-A, C-B, C-C, C-D + G-A (≈8 Mastodon lines stay: the streaming-host trust check) |
| **Result shape** (an SDK error or envelope around `send`) | Salesforce `query.rs`/`bulk_query.rs`/`ingest.rs` (144); Twilio `lib.rs:125-150` (25); Drive `client.rs:203-230` (26); Registry `client.rs:127-142` (16); Notion `client.rs:86-89` (4); Mastodon `error.rs:174-180` (7, the policy itself) | 222 | families A and B |
| **Paging / following** | Kraken `trades.rs:32-63` (32); Graph `client.rs:188-203,241-254` (30); Snowflake `client.rs:280-296` (17); Discord `api.rs:133-147` (15); Dropbox `lib.rs:80-94` (15); PokeAPI `lib.rs:207,218-229` (13) | 122 | family F; G-C for PokeAPI and Snowflake |
| **Validation** | Elasticsearch `lib.rs:182-263` (79: ≈10 lines turning `&str` into `IndexName`/`DocumentId`, the rest response correlation); OpenAI `lib.rs:374-419` (46: 9 lines of empty-id checks, 8 of filename rules, 6 of renames, 7 of a limited download); HubSpot `lib.rs:542-595` (54: 3 lines of empty-id check, ≈8 rebuilding the client per call for a fresh token, the rest batch attribution); Linear `lib.rs:276-291` (16: 3 lines of empty-id check) | 195 (≈40 of actual checks) | G-B for checks that restate Concord's segment rule; the rest stays |
| **Naming** | Discord `api.rs:119` (`anchor` = `set_anchor`, 1) | 1 | none needed |
| **Provider semantics / workflows** (correct SDK code: echo checks, builders, polls) | ClickHouse `client.rs:230-271,337-356` (60); Drive `upload.rs:281-299` (19); World Bank `lib.rs:274-292` (19); Strava `upload.rs:176-193` (18); Kraken `lib.rs:138-153` (16); B2 `lib.rs:186-195` (10); Xero `lib.rs:412-420` (9); Registry `blob.rs:88-95` (8); OpenAI `lib.rs:346-352` (7, forces `stream: false`) | 166 | stays: behavior |

Two observations shape the designs below.
1. **Pure** forwarders are rare. Most construction forwarders disappear once the wrapper has no
   reason to exist (C). What the wrapper *stores* is the remaining reason (G-A).
2. Naming glue is small. Re-exports with renames: Mastodon `lib.rs:97-102`, S3 `lib.rs:72-75`,
   Dropbox and Slack `…Config as ClientConfig`, PokeAPI and World Bank `type Client = …`, Twilio
   `type Error`. That is ≈20 lines. The large naming cost is that diagnostics name a *private*
   client (Twilio `lib.rs:57-65`, Registry FRICTION §F5). That goes away when the declared client
   is the public one, which C makes safe.

## G-A: Read accessors (typed access to declared values). *Smaller than you think*

**Fact it expresses:** none new. The charter's "behavior in Rust, with typed access to the facts"
applied to construction. Today the generated client has setters and navigation but **no getters**
(`emit.rs:1878-1912`: `new`, `with_base_url`, controls, navigation). Every wrapper therefore keeps
its own copy of what it gave to `new`: `http`, version, instance, site, region, endpoint, shop,
namespace, tenant. Eight wrapper structs store their own `http: reqwest::Client`:
- Shopify `context.rs:31` and `:114`;
- Salesforce `context.rs:49`;
- GitHub `context.rs:107`, next to a copied `base`;
- HubSpot `lib.rs:383`, next to `crm_base`;
- Strava `auth.rs:106`, next to `origin`;
- Distribution Registry `client.rs:55`, next to `base`;
- PayPal's `Builder` (`lib.rs:202`).

**Declaration (author writes):** unchanged. Generated, for every client and scope value (`Option<&T>`
for `x?: T`):
```rust
impl ShopifyAdmin {
    pub fn version(&self) -> &ApiVersion;
    pub fn http(&self) -> &reqwest::Client;
    pub fn base_url(&self) -> &concord::BaseUrl;
}
impl ShopifyAdminShop {
    pub fn shop(&self) -> &ShopDomain;
    pub(crate) fn access_token(&self) -> &String;   // #[sensitive]: only the SDK reads it back
    pub fn client(&self) -> &ShopifyAdmin;
}
```
Before, `examples/shopify-sdk/src/context.rs:28-34, 64-75`:
```rust
pub struct ShopifyContext { http: reqwest::Client, admin: ShopifyAdmin, version: ApiVersion }
pub fn with_version(mut self, version: ApiVersion) -> Self {
    self.admin = self.admin.set_version(version);
    self.version = version;                          // second copy, kept in step by hand
    self
}
pub fn version(&self) -> ApiVersion { self.version }
```
After: no struct. `admin.set_version(v)` and `admin.version()` are generated.

**Call site (caller writes):**
```rust
// before
let v = sdk.version(); let http = shop.http(); let site = dd.site(); let region = s3.region();
// after (the same calls on the generated handles)
let v = admin.version(); let http = admin.http(); let site = dd.site(); let region = s3.endpoint().region();
```

**SDK code that disappears:** ≈70 lines on its own; most of its effect is counted under C-B,
C-D and C-E, whose wrapper deletions need it.
- Shopify `context.rs` ≈35: the `http`/`version` fields in two structs (31, 33, 114-116), `with_version`'s double write (66-70), `version()` ×2, `http()` ×2, `shop()`.
- S3 `client.rs:262-270` (8). Datadog `client.rs:111-114` (4). Mastodon `lib.rs:152-155` (4). Registry `client.rs:105-108`, `159-162` (8). Kubernetes `namespace()` (4).
- The stored `origin`/`base` fields of Drive (`client.rs:50`) and Graph (`client.rs:66`).
- Salesforce's link validation reads `self.instance()`/`self.version()` instead of `self.inner.*`.

**Also removes glue in:** C (the stored copies are why wrappers exist), and E and F (SDK workflows and pagers read `client.version()` or `scope.namespace()`; a send policy gets the same values through C-E). With C-B and C-E together, Datadog's `site` is one value used as the base and as a policy input. Datadog FRICTION §6 gave up a stateful policy because its `site` field "could disagree with the base URL", and instead maps `received.url()` back to a site.

**Hardest counterexample:** secrets. A public getter for `#[sensitive] token: String` would let
any holder of a handle read the token back. So sensitive values' getters are `pub(crate)`: the
SDK's policy and workflows can read them, callers cannot. A second risk is name collisions between
a getter and a child navigation method of the same handle. Concord already reports those at the
declaration, naming both (§9).

**Simple control (Nager):** the declaration is unchanged. The generated client gains `http()` and
`base_url()`, two methods.

**Charter check:**
- C1: no words.
- C2: typed access to the facts.
- C3: no declaration change.
- C4: the getter name is the value name.
- C5: collisions are reported at the declaration.
- C7: `http()` makes stepping out easier.
- C9: compile-fail test that a downstream crate cannot call a sensitive getter.

**New concepts:** getters; `client()` on scopes; `http()`/`base_url()` on clients.

**Breaking changes:** a declaration whose value name equals a child navigation name on the same
handle stops compiling (not observed in the witnesses read; a scan of all 55 is needed).

**Why it might be wrong:** Every value name becomes public API that callers read, so renaming a
value breaks readers too, not only constructors. `pub(crate)` for secrets is one more rule a
reader must know.

## G-B: Build errors name the value. *Smaller than you think*

**Fact it expresses:** none new. Concord already refuses an empty, `.` or `..` path segment
(`__private.rs:420`) and carries `what` ("path segment `{id}`") and `reason`
(`error.rs:99-107`). Neither is public, so SDKs check the same rule *before* calling, only to get
their own error variant.

**Declaration (author writes):** unchanged. OpenAI forwarder before,
`examples/openai-sdk/src/lib.rs:396-401`:
```rust
pub async fn file(&self, id: &str) -> Result<Response<FileObject>, Error> {
    if id.is_empty() {
        return Err(Error::Invalid("empty file ID"));
    }
    self.api.file(id).send().await
}
```
Drive restates the rule exactly, `examples/google-drive-sdk/src/client.rs:205-207`:
`if id.is_empty() || id == "." || id == ".." { return Err(Error::invalid(..)) }`.

After: the forwarder is gone, and the SDK maps Concord's refusal once, in its existing
`From<concord::Error>` (`openai lib.rs:286-296`):
```rust
if let Some(invalid) = error.invalid_value() {         // Build errors only
    return Self::Invalid(invalid.what());               // "path segment `{id}`"; invalid.rule() == Rule::EmptySegment
}
```

**Call site (caller writes):**
```rust
// before
openai.file(id).await?;
// after (generated endpoint, nothing sent for an empty id)
openai.file(id).send().await?;   // Err(Error::Invalid("path segment `{id}`"))
```

**SDK code that disappears:** ≈50 lines.
- OpenAI `lib.rs:397-399, 404-406, 415-417` (9). The `file`/`batch` forwarders become unnecessary (≈12).
- Drive `client.rs:205-207` (3), HubSpot `lib.rs:543-545` (3), Linear `lib.rs:277-279` (3).
- Twilio `lib.rs:139-150` (12) + `TwilioError::EmptySid` (error.rs:99, 116, 141, 176: ≈12).

**Also removes glue in:** A (SDK error variants that exist only for these checks: Twilio `EmptySid`, OpenAI `Invalid("empty … ID")`).

**Hardest counterexample:** checks that are *stricter* than Concord's. OpenAI's filename rule
(`lib.rs:381-387`) and Elasticsearch's `IndexName::new` are SDK rules and stay in SDK types. The
design removes only restatements of Concord's own rules.

**Simple control (Nager):** unchanged. Nager's `ErrorKind::Concord` can now say which value
broke the path if it wants to.

**Charter check:**
- C1: no words.
- C5: the error names the user's value (`{id}`), never `__private`.
- C6: one accessor and one small enum.
- C9: one test per rule mapping value → `Rule`.

**New concepts:** `Error::invalid_value() -> Option<InvalidValue>` with `what()` and
`rule() -> Rule` (`#[non_exhaustive]`: `EmptySegment`, `HostLabel`, `HeaderText`, `Base`,
`Unformattable`).

**Breaking changes:** none (additive).

**Why it might be wrong:** SDKs may keep the early check on purpose, to fail at the call that
received the bad input rather than at `prepare()`. Publishing `Rule` freezes Concord's build rules
as API.

## G-C: Recognize returned URLs as declared endpoints (`E::from_url`)

**Fact it expresses:** the URL shape the declaration already states, read backwards. A
server-returned link (`nextRecordsUrl`, `resultUrl`, a PokeAPI `next`, a record's
`attributes.url`) is trustworthy exactly when it is a URL the declared endpoint could have built
from this client.

**Declaration (author writes):** unchanged. Generated for each endpoint type:
```rust
/// Builds this endpoint from a URL that `prepare()` of the same endpoint could have produced from
/// `client`. The checks:
/// - same origin and base path;
/// - client values equal to `client`'s;
/// - scope and endpoint values parsed with `FromStr`;
/// - only declared query keys, no repeats unless `each`;
/// - no user info, fragment or non-canonical path.
/// Relative references are resolved against the base. The error names the rule and the value,
/// never the URL.
pub fn from_url(client: &Client, url: &str) -> Result<Self, concord::UrlMismatch>;
```
Before, PokeAPI (a simple control), `examples/pokeapi-sdk/src/lib.rs:213-229`:
```rust
pub async fn follow(&self, next: &str) -> Result<Response<PokemonPage>, Error> {
    let list = self.list().prepare()?;
    checked_url(list.request().url(), next, false)?;
    finish(list.follow(next)?.receive().await?).await
}
pub async fn pokemon_url(&self, url: &str) -> Result<Response<Pokemon>, Error> {
    checked_url(self.list().prepare()?.request().url(), url, true)?;
    let detail = self.pokemon("placeholder").prepare()?.follow(url)?;
    finish(detail.receive().await?).await
}
```
plus `checked_url` (`lib.rs:233-268`) and `UrlRejection` (`lib.rs:124-155`). Before, Salesforce
`examples/salesforce-sdk/src/context.rs:140-149`:
```rust
pub fn query_link(&self, raw: &str) -> Result<QueryLocator, LinkError> {
    let role = LinkRole::QueryNext;
    let link = self.parse(role, raw)?;
    match link.rest.as_slice() {
        ["query", locator] => { /* … */ QueryLocator::new(*locator) /* … */ }
```
After: nothing in the SDK.

**Call site (caller writes):**
```rust
// before
let page = api.follow(next).await?;
let locator = ctx.query_link(raw)?;                        // then ctx.api().query().more_accounts(locator)
// after
let page = PokeApiList::from_url(&api, next)?.send().await?;
let more = SalesforceApiQueryMoreAccounts::from_url(&api, raw)?;   // `.send()`, or read `.locator()`
```

**SDK code that disappears:** ≈350 lines. Overlaps family F.
- Salesforce ≈250:
  - `context.rs:134-390` (249 → ≈30 remain: the documented `resultpages`/`resultPages` discrepancy SF-D2 and the record-type rule);
  - `error.rs:490-600` (`LinkRole`/`LinkErrorKind`/`LinkError`: 100 → ≈35 keeping the link roles);
  - the module docs that list the rules (`context.rs:1-23`).
- PokeAPI ≈95: `lib.rs:117-155` (37: `UrlRejection`, its `Display`, `Error::InvalidUrl`) + `207-268` (60: `follow`, `pokemon_url`, `checked_url`).
- Registry `client.rs:85-114` (`location`, 31 → ≈10).
- DigitalOcean `NextLink` filter-exactness checks in `pagination.rs` (not counted line by line).

**Also removes glue in:** F (link-following pagers: PokeAPI, DigitalOcean `NextLink`, Registry `Location`, Salesforce `nextRecordsUrl`) and D (the URL never appears in an error).

**Hardest counterexample:** query order. PokeAPI's server returns
`next = …/pokemon/?offset=1&limit=1` (observed in the fixtures, `tests/canary.rs:101`), the SDK's
test expects exactly that wire target (`tests/workflows.rs:36`), and the declaration lists `limit?`
before `offset?` (`lib.rs:86`). `from_url` accepts keys in any order and rebuilds them in declared
order: the same pairs, not the same bytes. It also refuses keys the endpoint does not declare. That
breaks PokeAPI's promise "follow a `next` URL exactly, retaining its query parameters and their
order" (`lib.rs:208`) unless the declaration lists the keys in the server's order (a provider fact
family K can state) and the SDK accepts that unknown keys are refused. Salesforce SF-D2 (the
documentation shows `resultpages`, the reference `resultPages`) keeps a 3-line SDK normalization.

**Simple control (Nager):** declaration unchanged. PokeAPI, a simple control, *loses* ≈95
lines, which is the right direction for C3.

**Charter check:**
- C1: no words.
- C4: one testable invariant: `E::from_url(c, u)?.prepare()?.request().url()` equals `u` up to key order.
- C5: `UrlMismatch` names the rule and the value.
- C7: `Prepared::follow` stays for links the declaration does not model.
- C8: no I/O.
- C9: property tests of that round trip, plus canary tests that the URL is never shown.

**New concepts:** `Endpoint::from_url`, `concord::UrlMismatch`.

**Breaking changes:** none (additive). It adds one associated function per endpoint type, which
counts against family H's word budget.

**Why it might be wrong:** Reading URLs backwards must mirror every encoding rule exactly (percent
escapes, trailing slashes, case), and any divergence refuses valid server links. It adds generated
surface to every endpoint for a need that ≈13 SDKs have.

## G-D: Nest by context, not by URL (no language change)

**Fact it expresses:** "a namespace is the context of every API group's namespaced resources",
which is the charter's idea 1 (a scope is a context, not a URL prefix). Kubernetes modeled URL order
(`core_v1().namespaced(ns)`, `apps_v1().namespaced(ns)`, …) and then wrote `NamespaceContext` to
bind one namespace across the four groups.

**Declaration (author writes):** `examples/kubernetes-sdk/src/api.rs:95-125`, before:
```rust
scope core_v1 {
    path "api/v1";
    /* cluster-wide endpoints */
    scope namespaced(namespace: NamespaceName) {
        path "namespaces/{namespace}";
        GET pods(..ListOptions) -> PodList { path "pods"; }
        /* … */
    }
}
scope apps_v1 { path "apis/apps/v1"; /* … */ scope namespaced(namespace: NamespaceName) { /* … */ } }
```
after (works today: `<scope>.x` references, AUTHOR_GUIDE §4.2):
```rust
scope core_v1 { path "api/v1"; /* cluster-wide endpoints, unchanged */ }
scope apps_v1 { path "apis/apps/v1"; /* … */ }

/// One namespace, across the API groups of this slice.
scope namespace(namespace: NamespaceName) {
    scope core { path "api/v1/namespaces/{namespace.namespace}";
        GET pods(..ListOptions) -> PodList { path "pods"; } /* … */ }
    scope apps { path "apis/apps/v1/namespaces/{namespace.namespace}"; /* … */ }
    scope batch { path "apis/batch/v1/namespaces/{namespace.namespace}"; /* … */ }
    scope networking { path "apis/networking.k8s.io/v1/namespaces/{namespace.namespace}"; /* … */ }
}
```

**Call site (caller writes):** `ctx.namespace(ns).core().pods()` → `kube.namespace(ns).core().pods()`.
The same shape, but the handles are generated.

**SDK code that disappears:** ≈50 lines. `examples/kubernetes-sdk/src/context.rs:109-152` (38,
`NamespaceContext` and its `Debug`) and `88-97` (≈10, `KubeContext::namespace`). With C-B
(`server: BaseUrl = concord::base!("https://kubernetes.default.svc")`), C-D (`token?: Bearer`) and
G-A, the rest of `context.rs:22-107` (76) goes too, leaving `context.rs` empty.

**Also removes glue in:** none beyond G.

**Hardest counterexample:** Kubernetes itself. Each group prefix (`api/v1`, `apis/apps/v1`, …) is
now written twice, once for cluster-wide resources and once under `namespace`. That repetition is
what family K's `prefix "…" { }` seed targets.

**Simple control (Nager):** unchanged.

**Charter check:**
- C1: the scope states a provider fact (namespaced resources).
- C3: unchanged.
- C4: every path is still visible on its scope.
- C6: one guide paragraph ("nest by context; reach an outer value with `<scope>.x`").

**New concepts:** none.

**Breaking changes:** for Kubernetes callers only: `core_v1().namespaced(ns)` → `namespace(ns).core()`.
Both trees could coexist during a migration.

**Why it might be wrong:** It trades a wrapper for a repeated path literal, which only family K
removes. Callers who think in URLs lose the `core_v1().namespaced(ns)` spelling that matches the
API reference.

## Seeds rejected in G

- **Endpoint aliases or renames in the declaration (seed 1).** A Rust item's name is not a provider
  fact (C1). The renames observed are OpenAI `create_batch`/`batch` over `batch`/`batch_status`
  and Discord `anchor`. They exist because a wrapper exists, and the declaration can already
  choose those names.
- **Tree-shaped generated names** (`s3_api::bucket::GetObject` instead of
  `S3ApiBucketGetObject`). They would remove the ≈20 lines of rename re-exports, but they rename
  every generated type in 55 SDKs, the simple controls included (C3).
- **Re-export conventions (seed 3).** About 2 lines per SDK (`pub use concord;`,
  `pub use concord_pagination as pagination;`). This is guide text, not a mechanism. The H09c
  import misses are better addressed by family F making `Resume` reachable from one place.
