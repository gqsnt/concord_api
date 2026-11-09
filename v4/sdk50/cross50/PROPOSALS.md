# Cross-50 analysis — what the 50 domains say about Concord

Status: **proposals, not decisions.** Nothing here changes Concord until the owner accepts it
(rule 7). Prototypes on `sdk50/x50-*` branches are compiled measurements, not adoption.

**At SDK50 close (2026-11-07):** these proposals and the prototypes are **inputs to the
desirability loop's hypothesis queue** ([`QUEUE.md`](https://github.com/gqsnt/concord_api/blob/archive/desirability-loop-1/v4/desirability/QUEUE.md) on the archive branch),
where each is pre-registered, measured and decided under the owner's instruction "finish → merge
PR #5 → loop". None of them was decided here. Mapping: Tier 1 → H04 (docs), Tier 2 → H11
(adoption), P1 → H09a, P2 → H10, P3 → H09b, M1/M2 → H06, M4 → H08, the removal table → H13,
`concord-ratelimit` status → H14. See `../CLOSING.md` § "In-flight work at close".

## How this was produced

- Input: integration `af6840c9`, 55 SDK packages (the 50 campaign SDKs plus five earlier ones).
  The declarations hold **63 clients, 91 scopes and 316 endpoints** (3,424 declaration lines).
- Six read-only theme sweeps, each over all 55 SDKs, their `FRICTION.md` (author),
  `FIRST_USE.md` (fresh docs-only consumer) and `MAINTENANCE.md` (independent maintainer):
  hosts/transport (a1), declared values and policy facts (a2), send policies and errors (a3),
  pagination (a4), generated surface and docs (a5), capability crates (a6). Each was requested
  and observed as `gpt-6.1-sol` xhigh (`MODELS.md`). Reports: `analysts/`.
- Brief (`analysts/_brief_common.md`): judge every change across all domains, prefer subtraction,
  name counterexamples, separate observed from inferred, and do not count historical complaints
  that current code already resolves.
- The coordinator spot-checked the removal claims (below). Line counts in the reports are
  source spans read statically; **no proposed change has been compiled yet** unless a row says so.

**Which complaints are current.** Waves 4–6 (SDK36–50) were built on `c3fd4f1e`, which already
contains the B34-D generated-doc changes (`32e7b0a7`). A complaint from those 15 consumers is
therefore about the current wording; a complaint from Waves 1–3 may describe wording that has since
changed. Both are listed; the current ones carry the weight.

## What Concord learned

1. **The core held.** No sweep found a domain that needs a new core concept. Every large
   abstraction the notes had suggested fails on a concrete counterexample from another domain
   (see "Rejected"). Client → scope → endpoint, declared values, `SendWith`, `Prepared`/`follow`,
   `Received::document`, `Error::delivery()`, the codec traits and the optional capability crates
   cover all 55 SDKs.
2. **The largest remaining cost for developers is how the generated interface explains itself.**
   Fresh consumers working only from rustdoc still mispredict the base/path boundary, cannot tell
   which crate's `Error` they hold, and read the wire output as the send result — including six
   consumers on the current wording.
3. **Several shipped capabilities are under-adopted.** `Retry::receive_with`, inherent
   `operation()`, declared result templates, `Error::kind()` and `document::<Binary, Bytes>`
   already do what several SDKs still hand-write. That is SDK subtraction with no Concord change.
4. **Only a few small additions remove repeated SDK surface**: a start position on `Pages`
   (17 public method pairs in 11 SDKs) and one transport constructor (22 SDKs). The other
   candidate additions save single-digit to tens of lines in one to four SDKs.
5. **True dead surface in Concord is small**, and two of the analysts' "unused" claims were wrong
   on inspection (see "Removal candidates").

## Ranked proposals

Reach counts SDKs; "removal" is SDK source unless marked. "Obs." = observed in source/notes;
"Inf." = inferred, not yet compiled.

### Tier 1 — generated documentation (macros only, no API change, all 55 SDKs)

| # | Change | Evidence (current-wording consumers in **bold**) | Counterexamples / limits |
|---|---|---|---|
| D1 | Print the declared **base** and the declared **path** separately, then the composed default URL, and say that `with_base_url` replaces the base including its path. | 6 consumers mispredicted the boundary: Airtable, **Cloudflare**, NASA, **Sentry**, **Spotify**, Zendesk; OpenAI's author lost `/v1` (a1 §1, a5). The B34-D label did not fix it for the three current consumers. | MediaWiki needs a full script URL; GitLab/Graph/NASA/Stripe keep prefixes in `base`. Do not reinterpret existing declarations — document them. |
| D2 | Name `concord::Error` (not bare `Error`) in generated `prepare()`/`with_base_url()`/`send()` prose. | 12 consumers: Adyen, **arXiv**, **DigitalOcean**, Dropbox, **Europe PMC**, Frankfurter, **Kraken**, **Sentry**, Square, **Strava**, Twilio, Xero (a5). | An SDK alias cannot make rustdoc print the alias in generated signatures. |
| D3 | When the client declares a send result, show that result first and describe the wire output as what prepared execution decodes. | 7 consumers: **Cloudflare**, Discord, **Kraken**, NASA, Postmark, USGS, Zendesk (a5 #1). 33 of 41 policy clients declare a result template (140 of 163 policy endpoints). | Kraken and Cloudflare unwrap envelopes: "Output: `Envelope<T>`" misstates what callers receive. |
| D4 | Move `validate_header_names()` below construction/navigation; hide its doc when the check is trivial (keep it callable). | 5 consumers: **arXiv**, **Europe PMC**, **Kraken**, Square, USGS. 24 clients in 23 SDKs have no native-name work (a5 #3). | Native `HeaderName` constants need the real check; removing trivial functions would break 12 checked-in assertions. |
| D5 | Emit implicit intra-doc links instead of ``[`F`](F)``, then drop the generator's blanket lint allowances. | One reported rustdoc failure (Europe PMC); the allowances hide the trap from every SDK (a5). | Not reproduced by the read-only sweep. |
| D6 | Stop describing policy values as "sent normally": policy values are sensitive by construction. | 13 client-value policies get the wrong sentence (`model.rs:572–592` vs `emit.rs:626–629`, a5). | — |
| D7 | Shorten repeated framework prose (macro inheritance, clone lifetimes, group-trait terminology) on the ordinary caller path. | 7 consumers: Adyen, **arXiv**, **DigitalOcean**, **Europe PMC**, **Kraken**, PayPal, Square; 632 request-summary placements for 316 endpoints (a5). | Keep `maybe_*`, `Sends`, groups and inherent setters — removing them restores forwarding code (a5 §6). |

Measurement for this tier is a compiled prototype plus a **fresh docs-only consumer re-run** on
SDKs whose current-wording consumers complained (Cloudflare, Sentry, Kraken), not a line count.

### Tier 2 — SDK adoption of what Concord already has (no Concord change)

| # | Change | Reach / removal (Inf.) | Counterexamples / limits |
|---|---|---|---|
| A1 | Replace the four `ErrorBodyError::{Metadata, Read, Decode}` copies with `concord::Error` + `kind()`. | Frankfurter, Datadog, REST Countries, Open-Meteo: **144 of 148 lines**, before caller migration (a3 #1). | Public pattern matching changes; existing tests match the variants. A second Concord stage enum would duplicate `ErrorKind` (a3). |
| A2 | Compare identities with inherent `Handle::operation()`, not `<H as Endpoint<T>>::operation()` or string names. | 7 SDKs incl. S3, ClickHouse, Datadog, Discord, Zendesk (a2 #4). Removes the output-type dependency; ~0 lines. | — |
| A3 | Name the send result in the five associated-type clients that can (Airtable, S3: `_`; Frankfurter, Nager, World Bank: concrete). | 5 SDKs; 0 runtime lines; better signatures (a3 #3). | Cloudflare, Kraken, MediaWiki project an envelope's inner type and stay associated. |
| A4 | Move Adyen and Xero to `Retry::receive_with`; document when to use `receive` / `receive_with` / `run`. | 2 SDKs, est. **15–30 lines** (a6 #1). | Spotify must keep `run` (it consumes a 429 document); B2's factory replaces upload targets. |
| A5 | `received.document::<Binary, Bytes>(limit)` where raw bytes and metadata are read together. | Europe PMC, Linear, Notion: 3 lines; replaces the `bytes_limited_with_meta` request (a3 #2). | Needs `concord-codecs` or the facade's `extras` feature. |
| A6 | Drop Kubernetes' forwarded group setters / `send_list` that predate group traits. | 1 SDK, ~38 gross lines (a4). | Stripe's item-ID association is a different custom trait. |
| A7 | Feature hygiene: OpenAI's unused `jsonl`, ClickHouse's unused default `json`. | 0 Rust lines (a6). | Dependency effect unmeasured. |

### Tier 3 — small additions that subtract repeated SDK surface

| # | Change (crate) | Reach / removal (Inf.) | Counterexamples / limits |
|---|---|---|---|
| P1 | `Pages::starting_at(Resume<K>)` on an unread traversal (`concord-pagination`). | **11 SDKs drop 17 public `_from` twins**, ~70 gross / ~53 net lines (a4 #3). DigitalOcean's maintainer named the duplication. | Mailgun validates the resumed cursor eagerly before returning the traversal; that check must move, not vanish. |
| P2 | One opt-in constructor returning a `reqwest::ClientBuilder` with redirects and native retries off (facade, beside the reqwest boundary). It names the transport contract `Error::delivery()` already assumes. | **22 SDKs**, 44 repeated setting lines, ~39 net (a1 #2, a6 #3). | S3/ClickHouse/Graph only disable redirects; GitHub/GitLab allow same-origin redirects; timeouts, proxies, TLS, decompression stay explicit. A supplied client cannot be certified. Not in `concord-auth` (no reqwest dependency there). |
| P3 | A fetched page whose continuation failed after its records were accepted (`concord-pagination`, additive constructor). | DigitalOcean directly; same shape in S3, Intercom, Graph, GitHub, Stripe (a4 #1). **No line saving** — representation only. | Kubernetes (other snapshot), Elasticsearch (partial search), Discord (unordered) must still reject the page. |

Lower in this tier (one to four SDKs, small savings; listed so they are not rediscovered):
repeated-key guard by projection (3 SDKs, ~18 lines; rejected once in W2 with 0 adopters), decoded
`Link` parameter (Sentry, ~16), streamed multipart (Mailgun, Strava, 20–30), `ReadError` Concord
accessors (Anthropic, Mastodon, ntfy, 10–18), `TokenCache::insert` seed (QuickBooks, Spotify,
8–15, must stay fallible), XML parse helper (4 SDKs, 25–35, would expose a parser type), selected
query-value comparison (DigitalOcean, ~8), read-only `base_url()` on clients (GitHub, Stripe, ~7).

### Tier 4 — macro opt-ins that remove misleading generated items (adds syntax)

| # | Change | Evidence | Counterexamples / limits |
|---|---|---|---|
| M1 | Let an author suppress selected generated controls (context setters, `with_base_url`) and generated `send()` on endpoints that need SDK target preparation. | Mastodon hides its generated client to protect an instance token; Xero hides tenant setters; Graph wraps; B2 and Registry hide placeholder-target sends; PayPal exposes an unconstructible `new` (a1 #5, a3, a2). **No line saving**; public surface becomes accurate. | QuickBooks relies on `set_connection` changing realm and credential together; S3 and Kubernetes rotate credentials. Never a global default. |
| M2 | A required runtime base (`base;`) with no generated default destination. | Mastodon, Elasticsearch, Salesforce, Snowflake, Registry (7 declarations); **net 0 lines** (a1 #4). | ClickHouse and Kubernetes defaults are useful; B2's problem is role-specific issued hosts. |
| M3 | Scope/endpoint-level `send` policy (today client-only). | 7 SDKs adapt around it (B2, Datadog, Elasticsearch, PayPal, Postmark, QuickBooks, Xero); **no net saving established** (a3 #4). | A policy value is sensitive by construction; QuickBooks uses one value as policy and realm path. |
| M4 | Mandatory typed policy classification, so a new endpoint cannot silently fall into a method-based rule. | 12 SDKs classify by HTTP method; Strava's maintainer showed omitted quota/scope/uncertainty rules compile (a2 #2). | Untested; the only candidate that could reject missing coverage, and the most syntax. |

M1 has the clearest need; M2–M4 stay open questions until a prototype shows a smaller interface.

## Removal candidates (Concord surface)

| Item | Claim | Coordinator check | Proposal |
|---|---|---|---|
| `concord_pagination::first_page_then` | 0 uses in 55 SDKs (a4) | Confirmed: 0 in `examples/`; used only in crate tests/checks | Demote in docs now; remove in a breaking release. |
| `Order::Completion`, `Options::failure_limit` | 0 SDK uses (a4) | Confirmed: 0 in `examples/` | Move to advanced docs; not proven dead. |
| `Error::meta()` (narrower than `head_meta()`) | 0 direct production uses (a3) | `error.meta()` appears in Registry tests; resolution to Concord vs SDK type **unverified** — a deprecation build decides | Deprecate in favour of `head_meta()` if the build shows no use. |
| `concord-extras` compatibility crate (6-line re-export) | 0 SDK uses (a6) | Confirmed for SDKs; the gate's compatibility checks (`checks/consumers/extras-*`, verify.sh steps) depend on it; published at 4.2.0 | Owner decision on a breaking release; tiny cost either way. |
| `Operation::is_endpoint()` | "no SDK production use" (a2) | **Wrong**: Stripe `src/send.rs:469` filters with it; 7 SDKs' tests assert it | Keep. |
| `Limiter::available_leases` | 0 production SDK calls (a6) | Used by `examples/native` | Keep. |

Do not remove (all have concrete users): `run`/`run_tokio`/`receive`/`receive_with`, `Prepared::follow`
(9 SDKs), `subdomain` (S3, Shopify, Zendesk), `maybe_*` (28 SDKs), `Sends` (7), parameter groups,
`PageItemEvents` (Drive), `OnError::Continue` (Snowflake), `IgnoreContentType` (10),
`Limiter::remove`, `observe_global`, `authorization(scheme, token)`, `TokenCache::get`.

## Rejected, with the counterexample that decides it

| Proposal | Why not |
|---|---|
| General "facts" API for policies (B34-V B) | Getters alone cannot reach a policy through `Endpoint<T>`; final-wire checks (Adyen encoded segment, Xero tenant header, signing, continuation binding) are not original-value equality (a2 #1, #3). |
| Generated getters on every handle (B34-V A) | Unused method growth and sensitivity/body exclusions; opt-in local readers have better evidence (a2, a5). |
| Universal host / URL-trust abstraction | Datadog changes parent domains; S3 binds signing region; B2 issues role-specific hosts (a1). |
| Universal list endpoint / generated pagination | Completion, snapshots, links and streamed rows differ; `Sends` + group traits already serve 5 generic walkers (a4). |
| Shared provider error enum or "error field means failure" | Cloudflare keeps diagnostics on `success:true`; MediaWiki rejects warnings; Linear/Shopify keep partial data (a3 #5). |
| `bytes_limited_with_meta`, a `DocumentError` stage enum | Already expressible (`document::<Binary, Bytes>`, `Error::kind()`) (a3). |
| Universal rotating-token transaction | PayPal has client credentials; Registry has realm challenges; Xero needs durable fail-closed state (a6 #2). |
| Merging rate limits into retry, or a default send layer | Discord, Intercom, Shopify and NASA disagree on what a quota is (a6 #7). |
| Nested bracket forms in codecs | One SDK (Stripe, 545 lines); would change `Form`'s flat contract (a6 #10). |
| Default `Encode::encode` refusal | Saves 8 lines and lets an incomplete encoder compile (a6). |
| Changing `SendWith::Output` to success/error types | Migrates every implementation, saves nothing; envelope projections need flexibility (a3). |

## Correctness concerns found in passing (SDK code, not Concord)

- **HubSpot and Strava may form an ownership cycle through `TokenCache`**: the stored acquisition
  future captures the owner that holds the cache (HubSpot `lib.rs:447–452`, Strava
  `auth.rs:182–186`); the cache README warns against exactly this. Inferred, not reproduced (a6).
  Xero, QuickBooks and Spotify capture inputs separately. Needs a test, then a two-SDK fix.
- `Prepared::follow` checks parse and origin only; it does not reject same-origin userinfo or
  fragments, which several SDKs reject themselves (a1). A tightening is a compatibility choice.
- QuickBooks `Error::status()` returns only Status-stage codes while decode failures carry a head
  status (a3). Needs the intended contract before calling it a defect.

## Next: compiled prototypes (branches, not merged)

1. `sdk50/x50-docs` — Tier 1 (D1–D7) in `concord-macros`; rustdoc `-D warnings` for all 55 SDKs;
   before/after generated pages; fresh docs-only consumers on Cloudflare, Sentry and Kraken.
2. `sdk50/x50-pages-start` — P1 with the 11 migrations; Mailgun's eager check kept.
3. `sdk50/x50-adopt` — Tier 2 (A1–A6) and the `Error::meta()` deprecation build.
4. `sdk50/x50-transport` — P2 with the 22 migrations.

Each gets an independent review; results and measurements are added to `RESULTS.md` here, and the
owner decides what enters Concord.
