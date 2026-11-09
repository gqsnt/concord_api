# Direction: construction — what is fixed and what may change (C1 + C2 + G1)

Saved from the [direction source](/home/canarit/projects/Concord_Next/work/design/directions/construction.md) and reviewed fixes. Original lane: Claude subagent, slot `lane-a`. Fix lane: Codex `gpt-6.1-sol` xhigh, as recorded in that source. Reviews: Codex `gpt-6.1-sol`; round 2 was an independent session, same provider.
Branch `direction/construction` at `b03e47e6` (base `ba10189b`; round-1 tree `9c1fa059`), pushed as `spike/dir-construction`. Worktree: `/home/canarit/projects/Concord_Next/work/wt/dir-construction`. Saved [measurement](/home/canarit/projects/Concord_Next/work/wt/dir-construction-measure.md) predates the fixes; final measurements below use round 2.

**Status: partly works.** The four reverted-controls failures FM1–FM4 reject. Checked construction also closes the reviewed Salesforce/Datadog bypasses and Zendesk/QuickBooks destination changes. DigitalOcean/Sentry destination gaps and 20 broken calls remain.

## Shape

Required client/scope values identify the handle. They have no setter unless declared `mut`. Optional and initialized values keep setters; endpoint setters remain. There is no negation statement.
`base x;` takes the base from a value implementing `AsRef<concord::BaseUrl>` and removes `with_base_url`. Literal bases keep that method (C3). Values get borrowed getters; clients get `http()`/`base_url()`, scopes `client()`. Sensitive values, policies and whole-header values have crate-only getters. Sensitive bases also hide `base_url()` and redact rendered URLs.

```rust
// Salesforce before: a fictitious base, replaced by OrgContext through with_base_url
pub client SalesforceApi(#[sensitive] token: String, version: ApiVersion) {
    base "https://instance.salesforce.invalid";
// after: the value base and checked version are construction facts
pub client SalesforceApi(instance: InstanceUrl, #[sensitive] token: String, version: SupportedVersion) {
    base instance;
```

Shopify marks `access_token` mutable, while `shop` stays fixed. Zendesk marks authorization/policy mutable and now uses a checked `Account` as its base. QuickBooks marks `connection` mutable and takes the base from it. Datadog uses `destination: Destination`, `base destination;` and checked keys. Salesforce requires `SupportedVersion`, enforcing MINIMUM on public `new` and Config construction.
FM1 `admin.shop(a, t).set_shop(b)`, FM2 `client.set_subdomain(x)`, FM3 `api.with_base_url(evil)`, FM4 `api.set_version(old)` → E0599 at caller code. External `api.token()` → E0624. Raw Salesforce versions now fail E0308. Rotation, `set_connection`, `set_policy` and fixtures remain valid.
Fixtures choose their base through `InstanceUrl::parse("http://127.0.0.1:8080")`, `Destination::at(site, url)` or checked connection/account constructors. Zendesk fixture origins are restricted to localhost/.localhost/.test. Native `reqwest::Client`, `prepare`, `into_parts`, `send_raw` and forward proxies remain.

## What disappears

Final round-2 classifier: **2,994 → 3,007** glue lines; negations remain zero. This supersedes round 1's **2,994 → 3,008 (+14)** on the same target set. The saved measurement's **3,402 → 3,416 (+14)** also includes untouched Mastodon (**408** on both sides). It is historical, not the final aggregate.
That saved table records construction 154 → 172 (+18), error 2,421 → 2,418 (−3), docs 27 → 26 (−1); result 59, operation 3, redaction 265, forwarding 206 and paging 267 unchanged. Model 4,013 → 4,159 (+146); behavior 5,278 → 5,257 (−21); other 1,875 → 1,881 (+6). DSL 297 → 302 (+5), endpoints 60 → 60, M4 4.95 → 5.03, negations 0 → 0. Source doctest blocks contribute to classifier totals.

| SDK | Saved glue | Saved model + behavior | Saved DSL / negations |
|---|---|---|---|
| datadog-events | 555 → 546 | 747 → 788 | 25 → 26 / 0 → 0 |
| quickbooks-online | 500 → 500 | 1,672 → 1,714 | 51 → 51 / 0 → 0 |
| salesforce | 941 → 969 | 3,162 → 3,168 | 81 → 85 / 0 → 0 |
| shopify | 337 → 332 | 2,107 → 2,123 | 29 → 29 / 0 → 0 |
| zendesk-support | 326 → 326 | 672 → 692 | 36 → 36 / 0 → 0 |

Pre-fix hand count: Salesforce `context.rs` −32/+13 and `ids.rs` −13/+19; Shopify `context.rs` −22/+11; Datadog `client.rs` −22/+13 plus `destination.rs` +32; Zendesk/QuickBooks one word each. Net **≈0 glue**. Salesforce loses `Inner`, `Arc`, copies and `ContextError::Base`; `InstanceUrl` wraps `BaseUrl`. No wrapper struct disappears: OrgContext keeps link validation; DatadogEvents keeps credentials/paging; ShopContext keeps version checks/bulk download. No final hand recount is reported.

## Simple case

Nager, PokeAPI and Frankfurter sources are byte-identical; DSL **8 → 8**, **7 → 7**, **11 → 11**. Calls and concepts stay fixed; tests pass. Generated `http()`/`base_url()` are additive.

## Costs

M3: `mut` on client/scope values, `base <ident>;`, public `concord::BaseUrl` and generated getters. The original hidden `__private::BaseValue`/`base_of` diagnostic paths were removed by the fix; diagnostics now name the public `String: AsRef<BaseUrl>` obligation.

| Compatibility surface | Complete recorded change |
|---|---|
| Required setters | **121 baseline in 54 crates; 117 removed, 4 retained through mut**. Optional/initialized setters remain. The earlier “121 removed” claim is superseded by both reviews. |
| Existing construction sites | **38 changed/deleted; 12 added**. These migrated sites are separate from the **20 remaining failures in 12 crates**. |
| Remaining generated-setter calls | compat `set_token`; mediawiki `set_user_agent` ×2; digitalocean `set_authorization`; postmark `set_token`; discord `set_token` ×2; usgs `set_endtime` ×2; europe-pmc `set_query`; cloudflare `set_policy` ×6; nasa/anthropic `set_api_key`; amazon-s3 source `set_signer`; native source `set_key`. |
| Remaining docs | **8 lines**: QUICKSTART, CALLER_GUIDE ×2, CHEATSHEET, MIGRATION, RECIPES, native README, cloudflare README. Use mut where rotation is a provider fact (Cloudflare policy, S3 signer, native key, compat token), otherwise construct another client. |
| Datadog | Removes `with_base_url`; `DatadogEvents::new` takes `impl Into<Destination>`; adds `Destination` and required `DatadogApiConfig::destination`; public construction requires checked key types. |
| Salesforce | Config requires `instance` and checked `SupportedVersion`; removes `ContextError::Base`; rewords `InstanceUrl` errors; removes identity setters and value-base replacement. Raw versions no longer type-check. |
| Zendesk / QuickBooks | Checked identity-backed bases remove generated destination replacement. Zendesk invalid accounts reject at construction; QuickBooks preparation now uses the connection's host. Replacement fixture connections must each select their destination. |
| Generated access / Debug | Reserves `http`, `base_url`, `client` and each value name. Scan found **0 collisions**, without compiling all **55** SDKs. Value bases appear under their value name in Debug; sensitive bases render `<redacted>`. Sensitive/policy/native whole-header getters become crate-only. |

M11: `construction_mistakes.rs`/`construction_base_type.rs` cover unknown/optional bases, `mut` on endpoint/optional values, getter/child collisions and a String base. Primary spans are authored tokens. The old E0277 “String is not a base URL” named private helpers; the reviewed fix exposes the public AsRef obligation instead.
M7 saved docs: **+228/−100 words** in crate docs/AUTHOR_GUIDE; no independent final word count. M8 compile time, expansion and performance remain unmeasured.

## What it rules out

A setter for a group without one carrier; arbitrary “same client, other host” for value bases; generic `with_base_url` on every client; generated method names colliding with values such as `id`, `name`, `version`. Optional credentials (`token?`) remain replaceable by design.
Mastodon's streaming host needs a second handle with `base origin;` and is not migrated. DigitalOcean/Sentry keep literal bases and credential-moving `with_base_url`. QuickBooks' native `SendWith` still rewrites official hosts for handwritten endpoints. Getter privacy must respect sensitivity rather than expose every value.

## Tests

Original touched set: **805 passed, 0 failed, 99 suites**; controls and QuickBooks `replace_connection` pass. Final retained logs: **867 passed, 101 suite summaries, 4 ignored**. Reviewers did not rerun them; no full-workspace gate or live proxy verification is reported.
`construction_fixed.rs` has **3 × E0599** caller failures and passing mut rotations. Wire tests cover fixture path prefixes, rotations/snapshots, getters/client/http and secret-free Debug. SDK doctests cover Shopify FM1, Zendesk FM2, Salesforce FM3/FM4/set_instance/token, Datadog and QuickBooks. Final external privacy, sensitive-base wire/error canaries and checked-construction boundary tests cover the review fixes.
Initial fixture-only edits: `values.rs`, `native_headers.rs` and one macro fixture gain mut; Zendesk's invalid-subdomain test constructs another client; Salesforce canary Config gains instance; Datadog uses Destination::at. Initially no assertion changed. The later fixes deliberately change these expectations:

| Existing expectation | Reviewed replacement |
|---|---|
| QuickBooks `tests/environment.rs`, `examples/replace_connection.rs`: prepare keeps the generated base snapshot | Prepared URL shows the replacement connection's host. Earlier-request snapshots, fixtures and send destinations remain tested. Native SendWith retains its rewrite. |
| Zendesk `first_use.rs`/`protocol.rs`: invalid subdomain fails at send | Fails at account construction with `InvalidInput { what: "account subdomain" }`. Zero-request assertions remain. |

Compile-fail snapshots would fail if forbidden methods returned. Original cases did not guard constructor bypasses; final checked-type tests do. Base diagnostics now avoid private helpers. No existing assertion was weakened.

## Lane's assessment

Two positive facts close the named FM/H06c cases for Datadog, Salesforce and Zendesk. `base instance;` states the real host; mut identifies rotation. The source records **≈0 glue** removed before fixes and no super-additive deletion. Wrappers retain behavior. The final reviewed construction rules work within their stated scope, with the remaining destination gaps, migrations and documentation costs recorded.

## Review

### Round 1

[REJECT](/home/canarit/projects/Concord_Next/work/reviews/direction-construction-review-1.md). CHARTER: C1=y C2=y C3=y C4=y C5=n C6=y C7=y C8=y C9=n.

- Blocking: sensitive base getters and Prepared Debug leaked base contents. Protect projections/rendering and add external-access/canary coverage.
- Blocking: native whole-header getters ignored runtime sensitivity flags. Restrict access and test from another crate.
- Blocking: public Salesforce/Datadog new/Config paths bypassed version/key validation. Require checked carriers on every path.
- Blocking: Zendesk/QuickBooks literal-base replacement moved credentials; DigitalOcean/Sentry had the same recorded gap. Carry checked destinations with identity.
- Blocking: 121 counted baseline setters, not removals; required Datadog destination and migrated caller accounting were omitted. Record the final surface separately from remaining failures.
- Non-blocking: base diagnostics exposed BaseValue/base_of. Use a public obligation.

### Fixes after round 1

`b03e47e6` makes sensitive-base getters crate-only and redacts URLs; restricts whole-header getters; requires SupportedVersion and checked Datadog keys; makes Zendesk Account and QuickBooks Connection own checked bases; removes private paths from diagnostics; corrects setter/caller accounting. Regression tests cover each fix. QuickBooks host snapshots and Zendesk validation timing change as listed above.

### Round 2 (last)

[ACCEPT_WITH_FIXES](/home/canarit/projects/Concord_Next/work/reviews/direction-construction-review-2.md). Independent session, same provider. CHARTER: C1=y C2=y C3=y C4=y C5=y C6=y C7=y C8=y C9=y. All round-1 implementation findings are fixed.

- Non-blocking: `docs/MIGRATION.md` still promises unchanged T4 call sites and the old prepared-base snapshot. It must describe the connection host and per-replacement fixture destination.
- Non-blocking: the direction summary still split old/new accounting. This record uses round 2: **117 removed / 4 retained; 38 changed/deleted / 12 added / 20 remaining**, including required Datadog destination.
- Recorded limits: DigitalOcean/Sentry destinations remain open; native QuickBooks SendWith retains T4 rewriting; Mastodon is unmigrated; 20 calls still break. No full-workspace gate, independent final doc census or M8 measurement. Final glue **2,994 → 3,007** supersedes the saved pre-fix table.
