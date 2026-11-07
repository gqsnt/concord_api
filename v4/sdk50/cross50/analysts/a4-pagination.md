The strongest pagination opportunity is to simplify traversal setup and shared guard mechanics while keeping continuation meaning in SDKs. A universal “list endpoint” abstraction would erase real differences in completion, snapshots, links, and streamed results.

I swept all **55 SDKs**, their listing declarations, pagination adapters, and the three assessment documents where present. **39 depend on `concord-pagination`; 36 construct `Pages`.** Kubernetes and Shopify expose page factories; Snowflake uses `independent`. Backblaze implements its own traversal, five SDKs keep paging manual, and ten have no pagination protocol in their implemented slice.

No project code, builds, or tests were run. Nothing was changed; no external sources were accessed.

**Highest-value findings and candidate changes**

The observations below are source-backed. Proposed APIs, migration reach, and savings are inference. Line savings count physical lines, including comments and blank lines; they are estimates of SDK source reduction, not performance claims.

1. **Separate a successfully decoded page from a refused continuation.**

   **Observed:** DigitalOcean decodes and splits a response, validates its next URL, and only then constructs `Page`. If validation fails, that page’s records never enter the traversal’s returned prefix: `digitalocean-sdk/src/pagination.rs:153–163`. Its maintainer explicitly records that resuming retries the same offending page: `MAINTENANCE.md:184–208`.

   This pattern also occurs with S3’s repeated/missing token (`client.rs:516–543`), Intercom’s invalid cursor (`pages.rs:61–75`), Microsoft Graph’s next-link refusal (`paging.rs:150–178`), GitHub’s refused link (`paging.rs:594–639`), and Stripe’s repeated edge ID (`pages.rs:463–493`).

   **Smallest candidate:** an additive constructor in **`concord-pagination`** accepting a fetched page whose continuation is `Result<Option<K>, E>`. It could deliver the accepted records and metadata, then report continuation failure without reporting successful exhaustion.

   **Reach / removal:** DigitalOcean is the clearest direct demand; at least five other adapters have the same shape. **No demonstrated source-line saving**—this is a representation improvement.

   **Public API:** a fetched-page result type and constructor; a continuation-failure representation. Avoid changing `Page.next` for every existing consumer.

   **Counterexamples:** Kubernetes rejects pages from another snapshot (`list.rs:305–308`); Elasticsearch rejects partial searches (`pit.rs:447–452`); Discord rejects unordered records (`api.rs:165–170`). Those records should remain rejected. Linear already preserves the entire offending response inside `Error::Page` (`lib.rs:332–336`), so “not yielded” does not always mean “unrecoverably discarded.”

   An unsafe URL also cannot automatically become `Resume<NextLink>`: several SDKs intentionally make `NextLink` constructible only after validation. The error should distinguish the last fetched page from a runnable next checkpoint.

2. **Share repeated-key bookkeeping, with separate pre-dispatch and pre-emission policies.**

   **Observed, pre-dispatch:** Datadog, Notion, and Mailgun remember requested keys and refuse another request for one already seen:

   - Datadog: `client.rs:160–169`.
   - Notion: `client.rs:148–155,178–185`, separately for query and children.
   - Mailgun: `events.rs:272,280–284`.

   Those four implementations contain **22 lines of guard bookkeeping/checks**, excluding imports and error definitions.

   **Smallest candidate:** an opt-in **`concord-pagination`** guard, conceptually `refuse_repeated_keys_by(project, on_repeat)`. Projection matters because Mailgun compares target URLs rather than its entire context-bearing cursor. An SDK error callback could preserve existing typed errors without adding a `PagesError` variant.

   **Reach / removal:** **3 SDKs, 22 gross lines**, approximately **18 net lines** after four configuration calls. Tracking must have an explicit finite bound or share a finite traversal cap.

   **Distinct observed mechanic:** Europe PMC checks the *returned continuation before emitting its page*, including longer cycles: `pagination.rs:47–64`. Its set, snapshot, and check occupy **10 lines**. Moving that check before the next request would change which records are accepted.

   **Counterexamples:** do not silently replace Europe PMC’s policy with the pre-dispatch policy. Do not globally reject empty/repeated terminal tokens before the SDK applies its termination rules. Standalone `fetch`/single-page APIs also need their own guarantees.

   Immediate next-equals-current checks occur in **17 SDKs**: S3, Elasticsearch, Drive, HubSpot, Intercom, Kubernetes, Linear, Notion, Salesforce, Sentry, Shopify, Stripe, Zendesk, GitHub, GitLab, Distribution Registry, and Microsoft Graph. A `Pages`-only guard would not safely eliminate all of them.

3. **Allow setting the initial resume position on an unread `Pages`.**

   **Observed:** **12 SDKs expose 18 start/resume method pairs**. The start-only forwarding functions occupy **73 lines**:

   Cloudflare 6; Datadog 3; DigitalOcean 27; Discord 3; Distribution Registry 6; GitHub 3; GitLab 3; Mailgun 3; Graph 3; Notion 6; Spotify 7; Stripe 3.

   DigitalOcean names this duplication directly: `MAINTENANCE.md:167–201`.

   **Smallest candidate:** **`concord-pagination`**, conceptually `Pages::starting_at(Resume<K>)`, accepted only before admission begins.

   **Reach / removal:** straightforward migration for **11 SDKs / 17 pairs**, removing **70 gross forwarding lines**, approximately **53 net lines** after configuration calls. SDKs could remove their `_from` public twins.

   **Counterexample:** Mailgun checks the resumed cursor’s original query and credential *before returning the traversal*: `events.rs:263–271`. Removing its twin without relocating that check would change eager validation. Xero likewise performs eager context checks, although it already takes a resume argument rather than exposing twins (`lib.rs:271–280`).

   This would reduce SDK methods; it would not change client → scope → endpoint.

4. **Add decoded access to a Link parameter, not another Link parser.**

   **Observed:** Sentry uses the shared parser, then manually strips quotes and unescapes parameter values: `pagination.rs:70–87`, **18 lines**. It asks for this precise accessor in `FRICTION.md:13–15`.

   **Smallest candidate:** **existing `concord-pagination::link` module**, a checked decoded-value accessor on `Parameter`. Currently the public representation exposes `raw_value`: [`link.rs:70`](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/integration/crates/concord-pagination/src/link.rs:70).

   **Reach / removal:** **1 demonstrated SDK**, roughly **16–17 net lines**.

   **Counterexamples:** Sentry’s `results` flag, duplicate-parameter refusal, cursor agreement and collection checks remain SDK code. Snowflake needs raw target spelling to reject empty targets (`links.rs:65–67`), so retain raw access.

5. **Use a small query-comparison helper before adding declaration-level “listing identity.”**

   **Observed:** DigitalOcean declares `tag_name` as a request filter and separately includes it in `FILTERS`. Every followed/resumed URL must carry exactly the same decoded values, including absence: `pagination.rs:22–24,148–150,180–194`.

   **Smallest candidate:** a URL-query comparison helper in **`concord-pagination`’s existing URL/link support**. It should compare selected decoded keys without rewriting the target URL.

   **Reach / removal:** **1 SDK**, replacing the 9-line comparison/helper spans at `180–182,189–194` with roughly one call: **about 8 lines saved**. The SDK still selects `"tag_name"`.

   **Public API:** a selected-query-values predicate. A macro attribute is a larger change than this evidence supports.

   **Counterexamples:** GitHub permits repository-ID/name path changes (`paging.rs:7–8`); GitLab permits project path/ID changes (`pages.rs:200–214`); Mailgun continuation URLs can add a path token (`events.rs:320–328`); Graph encodes context in opaque state. Requiring all initial query pairs or an identical path would reject valid continuations.

   Sentry is different again: it extracts a cursor and reapplies the original request’s filters, rather than following the target wholesale (`pagination.rs:108–135`).

6. **Keep the strict offset helper; do not broaden its default completion rule.**

   **Observed:** **7 SDKs use `next_offset`**: arXiv, Europe PMC, FRED, Kraken, QuickBooks, REST Countries, and USGS.

   Their surrounding rules differ:

   - FRED checks the echoed offset (`pagination.rs:47–49`).
   - Europe PMC references require stable hit count and full intermediate pages (`pagination.rs:133–157`).
   - REST Countries accepts an empty page beyond total before calling the helper (`model.rs:97–102`).
   - QuickBooks and USGS use one-based offsets without totals.
   - Spotify deliberately uses `offset + limit` when `next` exists, not returned length (`models.rs:143–153`).

   **Smallest change:** **docs only** explaining these compositions; **0 SDK lines removed, no public API change**.

   Kraken proposes `Pages::offsets` in `MAINTENANCE.md:172–177`. It could reduce constructor/projection scaffolding, but it cannot remove count extraction, missing-count errors, echoed-offset checks, differing strides, or context binding. I could not establish a net simplification across all seven.

7. **The incremental-row gap is real, but currently concentrated in USGS.**

   **Observed:** `Page<T,K>` owns `Vec<T>` ([`types.rs:17–24`](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/integration/crates/concord-pagination/src/types.rs:17)). USGS consequently has a separate CSV traversal with an owned opening future, reader, budget, counters, completion and page transitions: `export.rs:15–144`. The state/construction/reading spans alone occupy **110 lines** (`15–26,29–47,52–130`).

   **Smallest candidate:** an external **`concord-pagination`** reader-chain adapter whose continuation is selected only after validated EOF, while rows can be returned immediately.

   **Reach / removal:** **1 direct SDK**; up to **110 lines of machinery replaced**, but net savings remain unmeasured because the SDK still needs an opening factory and EOF projection.

   **Public API:** a named traversal of incremental readers, with typed incomplete outcomes and cancellation-safe polling.

   **Counterexamples:** Salesforce’s result-part reader validates declared counts and dialect at completion (`bulk_query.rs:25–34`); its existing job scheduler handles whole downloads. A generic adapter must retain late failures and partial rows, rather than collecting each reader into a `Vec`. Ordinary JSON listings gain nothing from this API.

8. **Several perceived gaps need documentation or SDK cleanup, not Concord features.**

   - **Locally detected failures appear as `PagesError::Provider`.** The facade maps *any factory error* to that variant (`pages.rs:144–150`), including local input, protocol and trust failures. Document “factory/adapter error” prominently; do not teach callers to interpret it as an HTTP/provider response. **0 source lines removed; no API change.**
   - **Metadata-preserving item traversal already exists.** `PageItemEvent::{Item,Page,Complete}` retains final metadata and empty-page events (`pages.rs:383–408`). Drive uses it in `tests/workflows.rs:377`. Prefer documentation over a new “last metadata” API.
   - **Backblaze’s custom loop is not evidence of a missing engine.** Its maintainer explicitly identifies existing `sequential` as suitable (`MAINTENANCE.md:139–162`). It currently returns `Ok(None)` both at completion and cap and abandons cancelled page attempts (`storage.rs:157–177`). Migrating it is SDK work with an intentional behavior review.
   - **Snowflake needs an already-fetched seed, not first-page discovery.** It preserves partition zero and schedules `1..count` (`partitions.rs:80–100`). A seedable independent facade could replace some wrapper code, but only one SDK demonstrates this need and its report semantics must survive. No shared addition is justified solely by the current line count.

**Per-SDK evidence table**

Paths below are relative to `examples/<SDK>-sdk/`. `F`, `U`, and `M` mean `FRICTION.md`, `FIRST_USE.md`, and `MAINTENANCE.md`; numbers are supporting lines. `—` means no relevant discussion found; `∅` means the document is absent. A mention can be positive or explicitly describe an excluded feature.

Costs are the **listed adapter/continuation spans**, not whole-SDK pagination totals. They include comments/blanks and exclude general transport, authentication, error formatting and most wire-model definitions. They must not be treated as automatically removable code.

| SDK | Listings and observed mechanics | Costed source evidence | Notes F / U / M |
|---|---|---|---|
| airtable | Records; opaque query `offset`; required cap; empty offset rejected, missing offset ends; HTTP metadata retained. No repeat guard. | `src/lib.rs:227–231,333–361` — **34 L** | F7–8 / U23 / M68–80 |
| amazon-s3 | `ListObjectsV2`; `IsTruncated` plus opaque token; missing/repeated token fails; common prefixes and response metadata retained. Reads initial token back from prepared URL. | `src/client.rs:495–546` — **52 L** | F161–164 / U11 / M16 |
| arxiv | Search and query; shared offset walker; feed total/size/start plus `next_offset`; required cap. Fetch-by-ID is a bounded batch, not a walker. | `src/lib.rs:93–145` — **53 L** | F13,17–19 / U12 / M13 |
| backblaze-b2-native | File-name listing; inclusive opaque name, fixed filters; custom remaining-budget/completion/resume state; empty page with continuation continues. | `src/storage.rs:76–89,131–216` — **100 L** | F151 is historical “no listing” / U72 / M139–162 |
| cloudflare-zones | Zones and DNS records; generic `Paging + Sends` walker; response page must match; `total_pages` decides continuation; rich page metadata. | `src/pagination.rs:55–141` — **87 L** | F123–125 / U42 / M37 |
| datadog-events | GET list is single-page; POST search has `meta.page.after` walker; body cursor replaced; pre-dispatch history guard; empty cursor error; warnings retained. | `src/client.rs:151–195` — **45 L**; GET declaration `src/api.rs:42–57` | F51–63 / U22 / M89,165 |
| digitalocean | Droplets, tagged droplets, actions, snapshots; body links followed; four resume twins; origin/path/filter revalidation; required cap; HTTP metadata retained, collection totals dropped. | `src/pagination.rs:26–195` — **170 L** | F8–10 / U152–156 / M165–208 |
| discord-rest | Guild channels are one complete response. Message history derives `before` from oldest ID; short page ends; checks strict order and size; resume twin. | `src/api.rs:119–178` — **60 L**; channels `:55–58` | F118–125 / U43–44 / M1 |
| distribution-registry | Catalog and tags; shared Link grammar; follow returned URLs even if size changes; context/path checks, immediate URL guard; resume twins; HTTP metadata. | `src/pages.rs:91–288` — **198 L** | F44–48 / U16 / M— |
| dropbox | List-folder then separate continue endpoint; cursor carries member/root context; `has_more` determines next; empty cursor rejected; default cap 1000; final cursor retained in metadata. | `src/lib.rs:114–164` — **51 L** | F36–46 / U17 / M10 |
| elasticsearch | PIT `search_after`; last hit’s exact raw sort array is key; latest PIT ID stored separately; empty hits end; partial search and repeated sort rejected; required cap. | `src/lib.rs:263–289`, `src/pit.rs:437–472` — **63 L**; PIT state `pit.rs:62–107` adds **46 L** | F75–86 / U61–71,177–182 / M42 |
| europe-pmc | Lite search JSON/XML: cursor history checked before emission. References JSON/XML: page numbers, stable hit count, exact page lengths. Citations fixed to first page; core metadata has no walker. Cap 1–20. | `src/pagination.rs:27–159` — **133 L**; first-page citations `src/lib.rs:103–108` | F9 / U17 / M5 |
| fred | Search and observations, XML/JSON, share `OffsetOptions + Sends + OffsetEnvelope`; echoed offset and arithmetic checked; required cap. ZIP CSV export remains one response. | `src/pagination.rs:9–56` — **48 L** | F14,60 / U9 / M164 |
| github | Link listings: issues, comments, labels, pulls/reviews/files, workflows/runs/jobs/artifacts. Operation/repo/parent-bound keys; stored-key restoration; URL guard; metadata and envelope totals. Workflow-specific runs is also declared, without a named convenience wrapper. | `src/pages.rs:47–198`, `src/paging.rs:333–379,561–640` — **279 L**; binding/storage `paging.rs:128–253` + Link wrapper `link.rs:122–175` add **180 L** | F71–89,161,208–211 / ∅ / ∅ |
| gitlab | Project issues; follows Link verbatim; bound collection; immediate URL guard; rejects missing next Link when `x-next-page` announces one; metadata; resume twin. | `src/pages.rs:85–216`, `src/link.rs:116–169` — **186 L** | F96–108 / U23 / M5 |
| google-drive | Files and changes; query tokens; required caps; immediate-token guard. Changes require exactly one next/new-start boundary; final polling token retained in page metadata. | `src/client.rs:142–198,229–289,300–313` — **132 L** | F92–106 / U13 / M35–37 |
| hubspot-crm | Contact search; JSON-body `after`; empty/repeated cursor rejected; total retained; optional cap. Batch read is not listing traversal. | `src/lib.rs:590–622` — **33 L** | F28–29 / U11 / M— |
| intercom | Contact list query cursor and exact-email search body cursor share projection/validation; empty/repeated cursor rejected; count/head metadata; required cap. | `src/pages.rs:19–79` — **61 L** | F14 / U7 / M12–26,78–85 |
| kraken-spot-rest | Trade history uses body `ofs`, fixed 50-page size, count, `next_offset`, default cap 20; fresh signing per page. Pairs/ticker/balances/open orders are finite responses. | `src/trades.rs:32–94` — **63 L** | F82–95 predates extension / U28 / M37–72,140–177 |
| kubernetes | Thirteen resource lists share a factory over `ListOptions`; key carries token and snapshot; checks snapshot/token progress; clears resource-version options on continuation. Uses `Page`, not `Pages`. | `src/list.rs:47–117,249–328` — **151 L** | F36–70,151 / ∅ / ∅ |
| linear | Issues GraphQL connection; `hasNextPage/endCursor`; malformed continuation preserves whole response in error; errors/extensions retained on accepted pages; optional cap. | `src/lib.rs:291–370` — **80 L** | F8 / U7 / M21 |
| mailgun | Events; body next URL; **empty items end even with links**; URL history checked before dispatch; cursor binds original query/credential; required cap and eager resume validation. | `src/events.rs:250–338` — **89 L** | F13–14 / U14 / M58 |
| mastodon | Public and hashtag timelines are manual pages; `max_id`, `since_id`, `min_id`; no Link walker or `concord-pagination`. Filters repeated by caller. | `src/api.rs:30–68` — **39 L** of shared options/list declarations | F149 / U45 / M45 |
| mediawiki-action-api | Search exposes whole continuation; category uses `Pages`; continuation is an indivisible string/integer map with reserved-key protection. Content batch separately merges answers across continued responses until all IDs are answered. | `src/pagination.rs:10–39`, `src/models.rs:24–77`, `src/batch.rs:146–184,188–214` — **150 L** | F24,63 / U20 / M55–59,99–110 |
| microsoft-graph | Users delta, groups, direct members; body next links; context/selection-bound persisted keys; next-vs-delta cardinality; immediate URL guard; final delta metadata; default cap 1000. | `src/paging.rs:59–197` — **139 L**; binding checks also `src/links.rs:55–60` | F54,74–90 / U29 / M8 |
| nasa-neows | Browse is manual zero-based page/size plus returned counters. Returned links intentionally ignored; no automated completion contract. Feed is finite. | `src/lib.rs:37–56,74` — **21 L** | F7 / U54 / M— |
| notion | Data-source query body cursor and block-child query cursor; separate history sets; shared cardinality/empty/immediate-repeat validation; response metadata; resume twins. Children traversal covers one level. | `src/client.rs:134–190,193–219` — **84 L** | F43,84 / U6 / M36 |
| open-library | Work search uses manual page/limit; author works uses manual offset/limit; author search exposes only `q`. No automatic traversal or total-based completion. | `src/lib.rs:34–39,42–44,58–60` — **12 L** of listing declarations | F78,96 / U7 / M59 |
| pokeapi | Manual limit/offset page plus body next/previous; explicit `follow` preserves returned query; validates collection/origin; no traversal engine. | `src/lib.rs:85–86,213–217,233–266` — **41 L** | F30 / U6 / M153–161 |
| postmark | Outbound search exposes one caller-selected count/offset page and total; no automatic walker. | `src/lib.rs:304–305` — **2 L** of listing declaration | F8 / U59 / M48 |
| quickbooks-online | Customer query; one-based position; short page ends, full page probes again; echoed position checked; `next_offset`; traversal drops provider time/page metadata. | `src/pages.rs:17–59` — **43 L** | F132 / U32–34,148–153,181 / M— |
| rest-countries | All five declaration shapes share `ListOptions + Sends`; `meta.more` plus checked offset/count arithmetic; accepts empty out-of-range page; traversal drops page metadata/demo marker. | `src/lib.rs:168–183`, `src/model.rs:91–107` — **33 L** | F12,78–95 / U24 / M16 |
| salesforce | SOQL first/query-more for Account/Contact/dynamic records; done/link cardinality, locator progress, record validation. Bulk sequential CSV sets use locator headers; parallel manifests discover validated result descriptors. | `src/query.rs:63–153,208–238,243–275`, `src/bulk_query.rs:611–675,723–768` — **266 L** | F5 / ∅ / ∅ |
| sentry | Issues/events use shared Link syntax, provider `results` flag and cursor; validates target/cursor agreement; reconstructs requests with original filters; immediate guard; required cap; head metadata. | `src/pagination.rs:15–136` — **122 L** | F7–15 / U15 / M110,130 |
| shopify | Products/variants/collections and locations/inventory connections, including nested connections; common connection-to-Page adapter; pageInfo progress checks; preserves GraphQL errors/cost. Factories, not `Pages` facade. | `src/connection.rs:52–85,242–290` — **83 L**; connection uses in `src/ops/catalog.rs:175–467`, `inventory.rs:57,172` | F108 / ∅ / ∅ |
| slack | Users; query cursor; missing/null/empty cursor ends; punctuation/padding preserved verbatim; short/empty intermediate page continues; rich page metadata; optional cap; no repeat set. | `src/lib.rs:178–201`, `src/models.rs:27–31` — **29 L** | F38–39 / U7 / M1 |
| snowflake | First response owns partition zero/descriptors; indexed independent plan for remaining partitions; shared Link parsing; validates row counts, links and handle; custom named wrapper/report retains unconsumed zero. | `src/partitions.rs:80–109,130–199,235–284` — **150 L**; Link policy `src/links.rs:50–113` adds **64 L** | F43–61 / U123 / M43 |
| spotify-web-api | Artist search, own playlists, playlist items share `PageParams + Sends + Listing`. Next link presence permits `offset + limit`; URL is not followed. Embedded playlist items expose a next offset. | `src/pages.rs:14–37,57–88`, `src/models.rs:143–154` — **68 L** | F16,106 / U40 / M65,145–146 |
| square-payments | Payments; opaque query cursor; absent/**empty** cursor ends; required cap; no repeated-key guard; traversal metadata `()`. | `src/lib.rs:124–140` — **17 L** | F12,20–21 / U36 / M46 |
| strava | Athlete activities; one-based pages; **empty**, not short, page ends; size/overflow checks; required cap; rates/head/attempt metadata. Upload polling is a separate mechanic. | `src/lib.rs:213–262` — **50 L** | F13–15 / U21 / M— |
| stripe | Seven list endpoints; forward/backward edge-ID cursors; template fingerprint and direction binding; embedded-list continuation; immediate guard; metadata; resume twin. | `src/pages.rs:126–193,279–405,419–440,456–494` — **256 L** | F122,166–184 / ∅ / ∅ |
| usgs-earthquake-catalog | GeoJSON: one-based offsets, short/no-data end, `next_offset`, required cap, metadata dropped. CSV: separate incremental reader traversal with late failures and its own budget/error vocabulary. | `src/lib.rs:97–128`, `src/export.rs:15–144` — **162 L** | F13,98 / U191–195,221,267–270 / M129–130 |
| world-bank | Indicator range; page counts choose continuation; returned page must match requested; page/provider metadata retained; optional cap; year helper reads one page. | `src/lib.rs:297–345` — **49 L** | F5,8 / U20 / M37 |
| xero-accounting | Invoices and modified-since invoices; cursor binds tenant/size/sync point; eager resume checks; strict modern metadata validation, legacy empty-page end; required cap. | `src/lib.rs:242–298,343–400` — **115 L** | F47,71,82 / U12 / M47–52,155–160,191–193 |
| zendesk-support | Tickets; `meta.has_more` decides completion; requires usable advancing `after_cursor`; checks optional body next-link cursor; reconstructs original request; required cap; metadata dropped. | `src/pagination.rs:23–61` — **39 L** | F76–95 / U9 / M7 |

These ten SDKs have **no pagination protocol in the implemented slice**. Their pagination adapter cost is **0 lines**:

| SDK | Evidence for absence | Notes F / U / M |
|---|---|---|
| adyen-checkout | Payment-methods is one complete response; payment/details/capture are transactions: `src/lib.rs:43–68`. | F92 / U77 / — |
| anthropic-messages | Create/count/stream messages only: `src/lib.rs:93–109`. | F103 / U62 / — |
| clickhouse | SELECT streams rows from one response; insert/command/ping are not paged listings: `src/wire.rs:39–71`. | — / — / — |
| frankfurter | Latest/historical currency pair reads: `src/lib.rs:42–50`. | — / — / — |
| nager-holidays | Complete holiday vector per country/year: `src/lib.rs:35`. | — / U9 / — |
| ntfy | Subscription/poll response streams and replay `since` cursor, not a finite page chain: `src/lib.rs:136–151`, `src/events.rs:517–548`. | F27 / U16 / M5 |
| open-meteo | One finite forecast response: `src/lib.rs:27–40,73`. | — / U5 / — |
| openai | Response generation/streaming, file retrieval and batch status/results; no list pagination: `src/lib.rs:33–51,408–430`. | — / U162 / — |
| paypal | Order/authorization/capture workflows, no listing: `src/lib.rs:79–168`. | F147 / U38 / — |
| twilio | Create/get message only: `src/lib.rs:33–42`. | — / — / — |

**Distinct mechanics that should remain distinct**

| Mechanic | Why a common default would be wrong | Smallest disposition |
|---|---|---|
| Opaque cursor omission versus empty value | Airtable rejects empty and present-null; Slack/Square treat empty as end; Kubernetes filters empty continue tokens. | **Leave to SDKs.** 0 removals/API changes. |
| Query cursor versus JSON-body cursor | Intercom supports both; Notion query and children differ; Dropbox continuation changes endpoint and body shape. | **Leave placement to SDKs**, reuse factory scheduling. |
| GraphQL connection cursor | Linear/Shopify need data/error/cost handling in addition to pageInfo. | **Leave envelope projection to SDKs.** |
| Edge-record ID cursor | Stripe direction changes which edge supplies the cursor; Discord additionally verifies strict record order. | **Leave to SDKs.** A generic equality guard cannot replace ordering checks. |
| Offset advanced by returned length | Seven `next_offset` consumers retain different validation and termination policies. | **Existing helper + docs.** |
| Offset advanced by declared stride | Spotify advances by `limit`, based on next-link presence. | **Leave to SDK.** Replacing it with length-based advancement changes behavior. |
| Page-number progression | Cloudflare/World Bank trust page totals; Strava and legacy Xero probe until empty; Europe PMC references require exact lengths/stable total. | **Leave completion selection to SDKs.** |
| Header Link followed wholesale | GitHub, GitLab and Registry retain returned query spelling and provider-controlled page sizes. | **Shared grammar; SDK trust policy.** |
| Header Link interpreted as cursor | Sentry checks `results` and cursor agreement, then resends its template. | **Shared parameter decoding; SDK interpretation.** |
| Body next URL | DigitalOcean, Mailgun, Graph and manual PokéAPI have different path and completion rules. | **Small URL predicates**, not a universal follow policy. |
| Continuation map | MediaWiki must replay all returned fields and prevent collisions with fixed inputs (`models.rs:24–77`). | **Leave to SDK.** A single string-token interface adds code. |
| Mutable PIT session plus sort token | Elasticsearch must keep the latest PIT ID even after a refused page and after traversal exhaustion (`pit.rs:443–452`). | **Docs for shared state**, not merely final metadata. |
| Independent partitions | Snowflake’s descriptors establish independent addressing; a total count alone does not. | **Keep `independent`; SDK validates the proof.** |
| Manifest-discovered download jobs | Salesforce discovers whole streamed result parts with separate completion evidence. | **Keep jobs engine**, distinct from record pages. |
| Continued batch merge | MediaWiki completes when every requested ID is answered; cursor alone cannot restore accumulated answers (`batch.rs:146–184`; `MAINTENANCE.md:99–110`). | **Leave to SDK.** |
| Incremental records across pages | USGS returns rows before the response finishes; next offset is selected after clean EOF. | **Possible reader-chain addition**, separately evaluated. |

**What Concord could remove or shrink**

- **Shrink the ordinary documentation surface around `Pages`, `Page`, `Resume`, caps and incomplete outcomes.** Put the generic engines in an advanced section. Current usage is sharply concentrated: 36 SDKs construct `Pages`; one directly constructs `independent`; Salesforce provides the manifest-job adapter exercised in `tests/continuations.rs:266`.

- **`first_page_then` is the strongest unused-entry-point pruning candidate.** I found **zero calls in the 55 SDKs’ source/tests**, and none elsewhere under `examples`. Its constructor and documentation occupy `independent.rs:63–128`, **66 lines**, with additional discovery-state machinery beyond that. Removing it would remove **0 SDK lines** and one public function; internal savings are unmeasured. Prefer demoting it in docs before a breaking removal. Snowflake explicitly cannot use its fetch-first/budget semantics (`FRICTION.md:43–53`).

- **`Order::Completion` and `Options::failure_limit` have no observed SDK source/test use.** They are candidates for advanced documentation, not proven deletions. **Do not remove `OnError::Continue`:** Snowflake exercises it in `tests/workflows.rs:309`.

- **Do not remove `PageItemEvents` for low adoption.** Drive’s final change token supplies a concrete correctness use, including empty final pages. Older complaints that item traversal loses metadata should point to this existing choice.

- **Do not delete the generic engines wholesale.** Salesforce’s application also uses `independent` and `manifest_jobs` (`examples/salesforce-app/src/sf_b.rs:533,574`), and GitHub’s application uses `resume_items` (`examples/github-app/src/audit.rs:502`).

- **Avoid adding generated pagination methods or another universal endpoint trait.** Generated parameter-group traits plus `Sends<O>` already support generic walkers in **five SDKs**: arXiv, Cloudflare, FRED, REST Countries and Spotify. Current emission is explicit in `concord-macros/src/emit.rs:44–96`; `Sends<O>` is defined in `concord-core/src/prepared.rs:264–266`.

- **Shrink obsolete SDK forwarding before expanding macros.** Kubernetes still forwards group setters and `send_list` (`list.rs:55–87,94–98`). The getter/setter forwarding occupies **38 listed lines**, although replacement bounds would reduce the net saving. Its older FRICTION proposals predate the currently available group traits. Stripe’s typed item-ID association and prepare-only adapter are a counterexample to blindly replacing every custom trait with `Sends`.

- **Keep raw parameter access and strict offset errors.** SDKs use those escape points to preserve stronger provider guarantees. REST Countries explicitly recommends retaining the strict helper and its local out-of-range exception (`FRICTION.md:78–88`).

**Limits and unresolved decisions**

- All behavioral conclusions are static readings. Existing test assertions were inspected where relevant but **not executed**.
- The per-SDK costs are reproducible source spans, not a complete executable-LOC census. Candidate savings exclude compatibility shims, documentation migration and new Concord implementation.
- Resume initialization, late continuation failure and repeat guards need explicit decisions about progress counters, cancellation, and calls made after polling begins.
- A generic context-bearing continuation could help Mailgun, Graph and Xero, but their contexts differ substantially. I could not establish that another shared public type would be simpler than their current domain types. It would add state/types to simple string-cursor SDKs; Square records that tradeoff in `FRICTION.md:12`.
- S3 and Europe PMC read configured values back from prepared requests (`client.rs:498–505`; `pagination.rs:97–101`); Spotify cannot validate its initial offset (`FRICTION.md:106`). This supports investigating readable generated parameter state, but not yet a pagination-specific macro API.
- Historical notes were reconciled against current code. In particular, PokéAPI’s `follow_with_limit` trial was explicitly reverted because it rewrote returned links (`MAINTENANCE.md:153–161`); it is evidence **against** making query rewriting a pagination default.