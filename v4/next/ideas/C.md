# C. Construction: what is fixed and what may change — merged ideas
Sources: CG-claude.md (§ C-A to C-E), CG-codex.md (§ C1 to C3). 8 lane designs → 6 entries: 2 KEEP, 4 KILL, 0 MERGED.

Family gate (owner): every KEEP closes at least one of FM1–FM4 (`v4/next/inputs/CONSTRUCTION_PROBLEM.md`)
without a negation statement. C1 and C2 together close all four. What stays open is listed so the
gallery sees it.

| Row | C1 `mut` | C2 base value |
|---|---|---|
| FM1 Shopify `set_shop` | closed | |
| FM2 Zendesk `set_subdomain` | closed | |
| FM3 Salesforce `with_base_url(evil)` | | closed |
| FM4 Salesforce `set_version(< MINIMUM)` | closed (the setter goes). Deleting `OrgContext` also needs `ApiVersion::new` to refuse old versions: today it is an infallible `const fn` (`salesforce-sdk/src/ids.rs:77`), and the check lives in `OrgContext::new` (`context.rs:94`) | |
| H06c: Adyen, Discord, Sentry, Datadog setters | closed | |
| H06c: Mastodon (private client), QuickBooks and Datadog `with_base_url` | | closed |
| Open: DigitalOcean, Sentry, Zendesk `with_base_url` (literal base + secret); Datadog `new` (keys checked together); USGS (an SDK test rule, no secret) | | C2 only if the SDK declares its base as a value; Datadog `new` needs a typed credential (C5, forwarded to D) |

## C1. Fixed unless declared `mut` — KEEP (rank 1)
Sources: CG-claude.md § C-A; IDEAS.md C seed 1.
Fact: the provider lets this value change during the handle's life (an OAuth token rotates, a connection is re-authorized). Every other required value is the identity of the handle.
Mechanism: a required client or scope value gets no generated `set_x` unless it is declared `mut` (Rust's keyword). Optional (`x?`) and initialized (`x: T = …`) values keep their setters. Variants: (a) required values only (claude); (b) every value, so Kubernetes writes `mut authorization?` (one rule, no exception, more `mut` words); (c) a `#[rotates]` attribute (IDEAS seed). Not reused: H06 arm A `#[fixed]`, the inverse default (it left FM3 open).
Declaration: Zendesk `lib.rs:43-51`, Shopify `api.rs:38-41`.
```rust
pub client Zendesk(
    subdomain: String,                                                    // no set_subdomain (FM2)
    #[sensitive] header("authorization") mut authorization: HeaderValue, // set_authorization stays
    mut policy: SendPolicy,                                               // see Breaking
) {
scope shop(shop: ShopDomain, #[sensitive] header(X_SHOPIFY_ACCESS_TOKEN) mut access_token: String) {
// Discord (token, limiter) and Salesforce (version): no edit; their setters disappear.
```
Call site:
```rust
admin.shop(a, token_a).set_shop(b);       // before: compiles, A's token sent to B (FM1). After: E0599
let b = admin.shop(b, token_b);           // after: enter the scope again
let shop = shop.set_access_token(fresh);  // same-shop rotation, unchanged
```
Glue removed: ≈0–20 lines directly (setter warnings in docs, e.g. `xero-accounting-sdk/src/lib.rs:142`). Its value is that C2 and G1 can delete wrappers without reopening FM1, FM2 or FM4: Salesforce `context.rs:36-133` (87 non-blank lines) and Datadog `client.rs:41-82` (41) need it. Checked: `emit.rs:851-891` generates one public `set_x` per value with no exception. Lane estimate for the rest.
Counterexample: QuickBooks replaces realm, credential source and environment together. They are already one value, so `mut connection: Connection` gives `set_connection` (with C2 the host follows too). Rotation in S3 and Mastodon uses `mut credentials` or the optional `authorization?`. Kubernetes keeps `authorization?`. Discord rotation means a new client with a new limiter, which is exactly its invariant.
Simple control: unchanged (Nager, PokeAPI and Frankfurter declare no client values).
New concepts: `mut` on a client or scope value.  Breaking: `set_x` disappears for required values that are not `mut`, in every SDK. Lane-observed callers: Zendesk and Cloudflare `set_policy` (tests), QuickBooks `set_connection`, S3 `set_signer`, NASA and Anthropic `set_api_key`. Correction: the lane's Zendesk fix `policy: SendPolicy = SendPolicy::default()` is refused today, because a send-policy value must be required (`concord-macros/src/model.rs:583`). Write `mut policy`, or lift that rule separately.
Shares mechanism with: G1 (accessors replace the copies wrappers keep), H1 (one setter doc fewer per fixed value).
Screen:
- C1: `mut` states a positive provider fact (the value changes). It is not a negation statement.
- C2: when to rotate stays in Rust.
- C3: the controls have no client values.
- C4: one rule, visible on the parameter. Variant (a) adds an exception for optional and initialized values: the owner picks (a) or (b).
- C5: a plain E0599 on the caller's line.
- C9: trybuild cases for FM1, FM2 and FM4.

## C2. The base is a declared value — KEEP (rank 2)
Sources: CG-claude.md § C-B; CG-codex.md § C1.
Fact: requests go to the host the caller names (a self-hosted instance, an org's instance URL), or to the host of a site or environment (Datadog site, QuickBooks or PayPal environment).
Mechanism: `base <value>;` names a client value whose type yields a checked base URL. Such a client has no `with_base_url`. `__private::Base` (`concord-core/src/__private.rs:67`, `#[doc(hidden)]` today) becomes public as `concord::BaseUrl`, and `concord::base!` checks a literal in an SDK impl. Variants: (a) the value sits in the parameter list (`base instance;`) and its type implements `concord::Origin` (claude); (b) the value is declared in the statement (`base instance: InstanceUrl;`) and its type implements `AsRef<BaseUrl>` (codex). Rejected variant: a host template `base "https://api.{site}"`. It keeps URLs visible but cannot express Mastodon, Salesforce or S3-compatible origins.
Declaration: Salesforce `api.rs:39-42`, QuickBooks `lib.rs:67-80`.
```rust
// before
pub client SalesforceApi(#[sensitive] token: String, version: ApiVersion) {
    base "https://instance.salesforce.invalid";
// after
pub client SalesforceApi(instance: InstanceUrl, #[sensitive] token: String, version: ApiVersion) {
    base instance;                                   // no with_base_url (FM3)
// QuickBooks: one value, three placements; the policy's host rewrite goes
pub client QuickBooks(mut connection: Connection) { base connection; path "v3/company/{connection}";
    send connection -> Result<_, Error>;
```
Call site:
```rust
ctx.api().clone().with_base_url(evil)?;   // before: compiles, the org's token goes to `evil` (FM3)
let api = SalesforceApi::new(http, SalesforceApiConfig { instance, token, version });
api.with_base_url(evil);                  // after: error[E0599]
```
Glue removed: the claude lane estimates ≈350–400 lines in ≈10 SDKs. The codex lane counts Mastodon alone at ≈50 gross lines, with "materially less" net. Checked (non-blank lines):
- Mastodon `lib.rs:119-150` (30), `152-176` (21), `242-247` (6) and `model.rs:20-67` (45). Its `check_origin` restates `Base::parse` (`__private.rs:75-99`) rule by rule, except "no path".
- Salesforce `context.rs:36-133` (87).
- Datadog `client.rs:41-82` and `97-124` (66).
- QuickBooks `lib.rs:157-168` and `policy.rs:35-37, 47-62` (31, the host rewrite).
- Elasticsearch `lib.rs:151-180` (28) and Registry `client.rs:74-95` (22).

These checked ranges total ≈335 lines. About 150 of them (Salesforce, Datadog) also need C1, and Mastodon's four forwarders need G1. Mastodon keeps its stricter origin rules and its streaming trust check. So ≈250–300 lines is the honest total for C2 with C1 and G1. It does not add up with G1. PayPal, Kubernetes, ClickHouse, Mailgun and Snowflake are lane estimates.
Counterexample:
- Mastodon's streaming host is a second origin advertised at run time. It becomes a second client (`base host;`), built by an ≈8-line SDK method that checks this instance advertised it.
- Fixtures: the SDK type decides whether it accepts loopback (Mastodon's `InstanceUrl` does). An enum such as Datadog's `Site` needs a fixture variant. Both lanes reject a `#[cfg(test)]`-only constructor, because integration tests are another crate.
- Proxies: a forward proxy on the `reqwest::Client` is unaffected. A reverse proxy is a new base at construction.
- Kubernetes and ClickHouse defaults: `server: BaseUrl = concord::base!(…)`.
Simple control: unchanged. Literal bases keep `with_base_url`, which the controls' docs promise for "another deployment".
New concepts: `base <value>;`, `concord::Origin` (or `AsRef<BaseUrl>`), public `concord::BaseUrl`, `concord::base!`.  Breaking: opt-in per declaration. An adopting client loses `with_base_url`, and its docs print `{instance}` instead of a placeholder host.
Shares mechanism with: G1 (wrapper deletion), H1 (no placeholder host on the request line; friction H7 and H8), K3 (Registry rebuilds a per-repository client with `with_base_url` today).
Screen:
- C1: a provider fact (self-hosted product, region, environment), not a negation.
- C2: no behavior. The value's type maps itself to a URL.
- C3: literal bases are unchanged.
- C4: `base x;` names the value. For an enum such as Datadog's `Site`, the URL table moves into a Rust impl, which a reader sees less easily than a literal.
- C5: a type that is not an origin is reported at `base x;`.
- C7: `prepare()` and `into_parts()` are unchanged.
- C8: the base is computed at `prepare()`.
- C9: compile-fail test that `with_base_url` is absent, and one wire test per site.

## C3. One context record projected into several placements — KILL (only moves glue; C4/C5)
Sources: CG-codex.md § C2; IDEAS.md C seed 3 (grouped values; rejected in CG-claude.md § Seeds).
Design: `context account: AccountingContext;` with `base account.base; path "v3/company/{account.realm}"; send account.connection`, and a setter only for the whole record. Each adopting client needs its own record type and checked constructor. Codex counts 12–20 added lines against ≈30 removed in QuickBooks (checked: `lib.rs:157-168` and `policy.rs:35-62`). QuickBooks's `Connection` already is the grouped value, so C2's `base connection;` gives the same atomic switch with no record. The projection `{account.realm}` collides with today's owner reference `{client.account}` (AUTHOR_GUIDE §4.2): a reader cannot tell a record field from an owner's value (C4, C5). Recorded limit: a group whose members feed different placements and must rotate together has no form under C1+C2 other than "fixed; build a new client".

## C4. Secrets stay on their origin (destination guard) — KILL (C4; variant b removes almost nothing)
Sources: CG-claude.md § C-C; CG-codex.md § C3.
Variants:
- (a) Implicit (claude): on a client holding a `#[sensitive]` or `send` value, `with_base_url` accepts only loopback, `localhost`, `*.localhost` and `*.test`.
- (b) Explicit (codex): an `audience session;` statement, with data-only `CredentialAudience`, checked at every execution route.

Screen:
- (a) breaks C4: it infers a destination rule from a redaction marker. A reader of `#[sensitive]` cannot predict that `with_base_url("https://proxy.corp")` fails; the lane itself calls it "a behavior change triggered by a redaction marker". It also closes FM3 only once Salesforce declares its base, which is C2.
- (b) removes almost nothing: 3 lines (PayPal `policy.rs:208-210`, checked), against 4–8 added (codex's own count).

Recorded limit: ≈121 lines of hand-written "loopback only, for tests" constructors stay (checked): DigitalOcean `lib.rs:69-96` (28), Graph `client.rs:94-127` (32), Drive `client.rs:88-105` (18), Xero `lib.rs:123-139` (17), Mailgun `lib.rs:107-118` (11), Snowflake `client.rs:91-105` (15). The H06c `with_base_url` rows of literal-base secret clients stay open too.

## C5. Credentials as Concord-owned typed values (`concord::auth::Bearer`, …) — KILL in C (family gate: closes none of FM1–FM4); forward to family D
Sources: CG-claude.md § C-D; IDEAS.md C seed 4.
Design: opt-in types in the sealed `ToHeaderValue` set (`Bearer`, `Basic`, `Authorization`, `Key`: redacted, no `Display`), plus an optional `header(..)` writer for credentials that write several headers (ClickHouse). It breaks no charter rule, but it closes no forbidden move: only the Datadog `new` row. Its glue belongs to family D: ≈180 lines of SDK token newtypes with redacted `Debug`, plus ≈120 lines of converting constructors (lane; checked: Zendesk `lib.rs:108-132`, 25 lines; Datadog `credentials.rs:27-82`, 53). The coordinator should hand it to the family D merge.

## C6. The send policy built from declared values (`send SigV4 { endpoint, credentials, clock };`) — KILL (C2; closes no FM)
Sources: CG-claude.md § C-E.
Design: the policy struct is built from named client values on each send. The PayPal witness moves behavior state into the declaration as client values with public setters: `tokens: TokenCache`, `reads/writes: concord_retry::Policy`, `refresh_margin: Duration`. That is retry and refresh configuration in the declaration (C2), and the lane names this risk itself. It closes none of FM1–FM4; it only fixes PayPal's unbuildable `new`. PayPal's `Builder` (`lib.rs:37-142`, 92 non-blank lines, checked) would mostly move into the policy's constructor rather than disappear.
