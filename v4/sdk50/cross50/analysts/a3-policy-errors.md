The strongest opportunities are to **shrink repeated error wrappers, improve result visibility using existing syntax, and document capabilities already present**. The evidence does not support moving provider success rules, replay decisions, or partial-result interpretation into one Concord policy.

This sweep covered all **55 SDKs**, using source and documents only. No builds, tests, SDK execution, external services, or file changes were performed.

**Observed inventory**

- **39 SDKs** implement `SendWith`; **16 do not**.
- There are **42 written implementation blocks**, occupying **1,457 physical lines**, including comments and blank lines. S3’s seven-line macro body expands into four implementations, giving **45 implementations after expansion**.
- All those implementations return a `Result`.
- There are **41 client `send` declarations**: **33 explicit result templates** and **8 associated-type results**.
- **41 SDKs** call `Received::document`; **11** call `bytes_limited`; **6** explicitly inspect `Error::delivery()`.
- The theme appears in **all 55 SDKs**. Absence below therefore refers to a particular mechanism, especially `SendWith`, rather than the entire theme.

Line counts below measure identified source blocks. They are **not automatically removable lines**. Proposed savings are separately labelled as inference.

**Most valuable findings**

1. **Four SDKs reproduce an error-stage enum already represented by `concord::Error::kind()`.** Frankfurter, Datadog, REST Countries and Open-Meteo contain **148 lines** defining `ErrorBodyError::{Metadata, Read, Decode}` and forwarding conversion/formatting implementations. Datadog and REST Countries explicitly identify the Frankfurter copy. Replacing these blocks with four aliases to `concord::Error` would delete **144 lines before caller migrations**, without adding a Concord API. It would change public pattern matching, so this is a migration candidate, not a transparent refactor. Sources: Frankfurter `error.rs:27–63`; Datadog `error.rs:26–65`, `FRICTION.md:65–76`; REST Countries `error.rs:77–116`, `FRICTION.md:64–75`; Open-Meteo `error.rs:17–47`.

2. **A proposed raw-bytes-with-metadata helper is already expressible.** `received.document::<Binary, Bytes>(limit)` returns an owned head beside bounded raw bytes. `Binary` accepts any content type and returns the bytes unchanged. This addresses Europe PMC’s requested `bytes_limited_with_meta` capability without another method. Straightforward two-line-to-one-line substitutions exist in **Europe PMC, Linear and Notion: three SDK lines saved**, plus fewer header-map clones on successful reads. Other raw-body users need additional adaptation. Sources: [Binary implementation](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/integration/crates/concord-codecs/src/binary.rs:31), [document implementation](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/integration/crates/concord-core/src/response.rs:288), Europe PMC `lib.rs:51–52`, Linear `lib.rs:229–230`, Notion `client.rs:229–230`.

3. **Five of eight associated-type declarations can name their results using existing syntax.** Airtable and S3 can substitute `_`; Frankfurter, Nager Holidays and World Bank can name their fixed result types directly. Only **Cloudflare, Kraken and MediaWiki** require a generic projection from an envelope’s inner type. This is primarily a discoverability problem, with **zero necessary runtime-line removal**. The substitution implementation preserves concrete templates; it does not require an underscore. Sources: [template substitution](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/integration/crates/concord-macros/src/emit.rs:1768), the eight declarations listed below, Cloudflare `FRICTION.md:8–25`.

4. **Client-wide `send` causes genuine adaptation, but the adaptations differ.** QuickBooks moves realm-bound authorization into the client; Elasticsearch and Postmark dispatch by output traits; Datadog dispatches by generated operation identity; PayPal separates its token client; B2 hides inappropriate generated send paths. A narrowly inherited scope/endpoint policy could improve representation. The evidence does **not** establish a large net line saving: provider rules and authentication boundaries would remain. Sources: [client-only parser restriction](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/integration/crates/concord-macros/src/syntax.rs:993), QuickBooks `FRICTION.md:9–39`, PayPal `FRICTION.md:9–25`, Datadog `MAINTENANCE.md:126–131`, B2 `FRICTION.md:48–59`.

5. **“HTTP success”, “provider success”, and “usable partial result” must remain separate.** Cloudflare accepts diagnostic `errors` beside `success:true`; MediaWiki rejects warnings; Linear and Shopify retain partial GraphQL data; Elasticsearch returns an error containing a complete bulk report; Postmark returns successful batch transport with rejected items. A shared “error field means failure” rule would weaken these contracts.

**Per-SDK evidence table**

Paths after each SDK name are relative to `examples/<name>-sdk/src/`. `D` means an explicit declared result template; `A` means associated-type result; `—` means no `SendWith`. `B—` means no separate provider-error/partial-result discriminator inside a successful body was found in this implemented slice; ordinary collection envelopes may still exist.

Notes use `F/U/M = FRICTION.md / FIRST_USE.md / MAINTENANCE.md`, with line numbers. `missing` means the document is absent; “unchanged” means the maintainer mentions the existing path without adding a new mechanism.

| SDK | Send implementation, ordered behavior and measured cost | SDK errors and successful-body behavior | Notes F / U / M |
|---|---|---|---|
| **adyen-checkout** | `policy.rs:289–462`, **174**, D. Prepare; constrain destination; validate key/body; reserve replay identity; retry under retention checks; preserve report; check guarantee; bounded document; decode and confirm response. Detailed trace below. | `error.rs:128–141`, **14**: Concord, Provider, Reconciliation and local failures. `models.rs:185–230`, **46**: payment refusal/pending/action states remain data. | 39–43 / 58–60 / 62,101 |
| **airtable** | `lib.rs:204–234`, **31**, A. 207 receive; 208–224 non-2xx raw bounded body plus optional parsed envelope; 226 decode with head; 227–230 reject empty offset. | `lib.rs:96–122`, **27**: Concord with head, HTTP with raw/parsed/read outcomes, EmptyOffset. Provider error accepts object or string, `73–84`, **12**. B—. | 6 / 48 / 73–74 |
| **amazon-s3** | `client.rs:107–113`, **7** written, four expanded impls, A. Prepare → sign → specialized signed send. Shared status mapping `383–403`, **21**; bounded XML error read `error.rs:264–304`, **41**. | `error.rs:490–523`, **34**: build/signing/transport/API/304/response/protocol/body distinguished. B—; streaming integrity and length are separate guarantees. | 106–112 / 19,89 / 50–55 |
| **anthropic-messages** | `lib.rs:150–165`, **16**, D. 154 prepare/receive; 155–161 non-2xx → bounded JSON document; 162–163 decode with metadata. | `error.rs:26–42`, **17**: Api, Concord, Incomplete, InvalidRequest. HTTP-200 stream error becomes incomplete output with partial message: `stream.rs:207–229`, **23**, and `479–482`, **4**. | 14–17 / 24,57 / 53 |
| **arxiv** | `policy.rs:43–91`, **49**, D. 46–61 re-read and validate page arguments; 62–70 lease/pacing; 71 receive; 72–75 bounded Atom HTTP failure; 76–79 successful-feed problem; 80–87 echoed paging checks. | `error.rs:30–43`, **14**: HTTP failure and Atom API failure are distinct. A 2xx error feed is not a successful page. | 10–12 / 74 / 75–79,125 |
| **backblaze-b2-native** | `wire.rs:82–92`, **11**, D; helper `96–101`, **6**. Receive; non-2xx bounded JSON; otherwise value+head. Upload/download have separate prepared paths. | `error.rs:450–466`, **17**: provider/contract/checksum/concord/input/deadline. `storage.rs:237–249`, **13**, classifies uncertain delete. `upload.rs:774–787`, **14**, combines delivery and source progress. B— for provider error envelopes. | 48–59,140–143 / 37–38 / 52–64 |
| **clickhouse** | **—**. `client.rs:366–389`, **24**, bounded non-2xx text; `393–433`, **41**, exception in 2xx head/body; `444–468`, **25**, opening checks. | `error.rs:21–43`, **23**. `rows.rs:94–113`, **20**, maps a tagged stream exception while retaining rows-before-error and metadata. Clean HTTP EOF alone does not prove query completion. | 38–57 / 22 / 13–21,44–46 |
| **cloudflare-zones** | `policy.rs:156–199`, **44**, and `209–240`, **32**, A. Both receive/retry then decode; `Failure` → Provider; success projects inner result; missing result → Envelope. Shared receive `98–136`, **39**. | `error.rs:210–236`, **27**. Two-phase discriminator parsing `wire.rs:123–176`, **54**. `success:false` wins over an ill-shaped result; `success:true` retains errors/messages. | 8–45 / 49 / 12–14 |
| **datadog-events** | `api.rs:85–114`, **30**, D. 89–97 identify posting operation and error mapping; 98 receive; 99–102 retain responding host; 103–110 require posting status 202; 112 reject. Rejection helper `116–135`, **20**. | `error.rs:277–307`, **31**: authentication, bad request, rate limit, provider, protocol, uncertain post, Concord. Nonfatal partial-data warnings retained: `model.rs:139–141`, **3**. | 65–76 / 29,185–192 / 64,126–131 |
| **digitalocean** | `lib.rs:112–118`, **7**, D; finish `121–134`, **14**. Receive → bounded typed non-2xx failure or decode with head. | `error.rs:66–85`, **20**. Provider carrier `42–48`, **7**; class derives from authoritative status. B—. | 8–12,40–47 / 47 / 49–55 |
| **discord-rest** | `limiter.rs:357–405`, **49**, D. Prepare route/resource; admit once; receive once; observe headers; 429 has separate document; other non-2xx gets API document; successful write decode failure is ambiguous. | `error.rs:280–299`, **20**. Headless-write mapping `limiter.rs:93–103`, **11**. No automatic resend. B—. | 23–29,103–109 / 33–36 / 34–38,73 |
| **distribution-registry** | **—**. `client.rs:147–219`, **73**, answers one trusted bearer challenge using cloneable bytes and token cache. Error conversion `error.rs:748–784`, **37**. | `error.rs:30–52`, **23**. Parsed/empty/unrecognized/oversized/unreadable error bodies remain distinct, `323–337`, **15**. Expected statuses and upload recovery stay operation-specific. B—. | 103–111 / 18,32 / 4–6 |
| **dropbox** | `wire.rs:52–69`, **18**, D. 56 receive; 57–61 retain head, bounded raw bytes and parsed error; 63–67 decode with head. | `error.rs:256–271`, **16**. ErrorBody retains both raw bytes and provider union; `208–242`, **35** shown parsing/access/formatting span. B—. | 9–21 / 55–57 / 68 |
| **elasticsearch** | `lib.rs:110–117`, **8**, D. Receive → `T::read`. Dispatch trait `123–135`, **13**; standard error/decode path `137–149`, **13**. Document overrides in `document.rs:246–273`, **28**, and `308…`. | `error.rs:17–50`, **34**. Bulk correlation `bulk.rs:616–666`, **51**, checks count/action/id/errors flag and returns `PartialBulk(report)`. Search partiality `pit.rs:447–453`, **7**. | 12–14 / 23 / 41 |
| **europe-pmc** | `lib.rs:43–59`, **17**, D. Prepare; extract requested page size; receive; non-2xx bounded raw body with cloned head; otherwise decode and validate page. | `error.rs:48–63`, **16**; raw HTTP carrier `8–14`, **7**. Provider XML/JSON envelope and page validation remain local. B—. | 8–11 / 29–34 / 51 |
| **frankfurter** | `lib.rs:76–91`, **16**, A. Receive; bounded JSON problem, map body-stage error; decode rate+head. | `error.rs:101–111`, **11**. Duplicate stage wrapper `27–63`, **37**. Provider body’s optional status is advisory. B—. | 24–32 / 37–48 / 35 |
| **fred** | `lib.rs:37–63`, **27**, D. Receive; non-2xx parse Retry-After then bounded `ProblemDocument`; otherwise declared decode. | `error.rs:41–58`, **18**. HTTP carrier `29–37`, **9**, retains typed document and retry-header parse result. Error format handles provider XML/JSON; no body-error promotion on 2xx. | 9–12 / 7–9 / 73,86 |
| **github** | **—**. `meta.rs:226–292`, **67** including docs/helpers: edited send, send with metadata, conditional send; 304 gets `NotModified`, otherwise native decode with metadata. | `error.rs:32–80`, **49**. API variant retains Concord error, not a typed GitHub error document. B—. | 32–36,98–103 / missing / missing |
| **gitlab** | `send.rs:31–38`, **8**, D. Receive → finish `42–48`, **7**; decode `51–62`, **12**; error document/read classification `66–90`, **25**. | `error.rs:19–39`, **21**. Provider document ignores media label; malformed bytes retained as Unrecognized, limit/read distinct. B—. | 30,85 / 24,39 / 19 |
| **google-drive** | **—**. `client.rs:126–140`, **15**, receive/finish; `error.rs:153–187`, **35**, bounded media-agnostic provider document and upload-session 404 classification. | `error.rs:80–101`, **22**, kind-based object with metadata/provider/source accessors. Raw page retains `incomplete_search`, `model.rs:196`; traversal refuses it, `client.rs:184–188`, **5**. | 39–44 / 6,19 / 27–41 |
| **hubspot-crm** | **—**. Native generated send. `lib.rs:575–587`, **13**, uses `Delivery::NotSent`, definite 4xx except 408, otherwise CreateUncertain. Refresh is separate. | `lib.rs:329–345`, **17**. Batch attribution `255–299`, **45**, correlates IDs, preserves unmatched results and unattributed errors, and produces `Unreported` per requested ID. | 11–16 / 33–34 / 32,44 |
| **intercom** | `lib.rs:54–68`, **15**, D. Receive; bounded typed JSON failure; decode value+head. | `error.rs:136–145`, **10**. Provider carrier `106–111`, **6**. B—. | 10,15 / 7–8 / 144–145 |
| **kraken-spot-rest** | `policy.rs:146–151`, **6**, A. Delegate to signed preparation and send. Signing `54–94`, **41**; final handling `124–143`, **20**. | `error.rs:167–180`, **14**. Non-2xx → HTTP; 2xx nonempty error list → API; absent result → MissingResult. Signature owns nonce and exact encoded bytes. | 19,34 / 66–73 / 37–39 |
| **kubernetes** | **—**. Native errors converted by `error.rs:35–63`, **29**, using bounded `IgnoreContentType<Json>` Status read. | `error.rs:129–137`, **9**; list/watch add their own error enums. HTTP-200 watch `ERROR` is a typed event: `watch.rs:128–130`, **3**, requiring caller handling. | 103–104 / missing / missing |
| **linear** | **—**. `lib.rs:223–242`, **20**: receive; non-2xx retain raw body and parsed GraphQL diagnostics; otherwise envelope+head. | `lib.rs:169–198`, **30**. Envelope `140–149`, **10**, retains data/errors/extensions together. Partial pages may succeed; unusable connection becomes `Error::Page` retaining the response. | 7–9 / 14–16 / 1–3 |
| **mailgun** | `policy.rs:85–91`, **7**, D; execute `94–116`, **23**. Determine POST; map headless/unreadable-success failure to AmbiguousSend; bounded non-2xx document; retain metadata. | `error.rs:106–118`, **13**. Non-2xx stays Provider, including server error; no automatic replay. Delivery/processing failure in event data remains domain data. | 13 / 89–90 / 138 |
| **mastodon** | `error.rs:171–181`, **11**, D. Receive; bounded API read; otherwise decode. Read helper `31–42`, **12**. | `error.rs:101–111`, **11**. Provider document becomes `Option`; body-read failure detail and full head are discarded intentionally. B—. | 23 / 49–55 / 109 |
| **mediawiki-action-api** | `policy.rs:10–40`, **31**, A. Receive; bounded non-2xx document; decode envelope+head; error→Api; any warnings→Warnings; absent query→InvalidResponse; project data/continuation. | `error.rs:39–57`, **19**. Distinct HTTP, API, warning and invalid-envelope outcomes. | 6–18 / 35,244 / 63 |
| **microsoft-graph** | **—**. `paging.rs:110–133`, **24**: receive; bounded media-agnostic error envelope; distinguish malformed/unavailable document; require 200; bounded successful decode. | `error.rs:148–179`, **32**. Successful envelope then validates object and next/delta-link cardinality, `paging.rs:134–149`, **16**. B— for embedded provider error. | 61–63 / 38,81 / 35 |
| **nager-holidays** | `lib.rs:62–71`, **10**, A. Receive → decode with head → HolidayResponse. | `error.rs:15–32`, **18** for kind and object. Classifies HTTP 400 specially; keeps native unread error response rather than parsing provider documents. B—. | 43–48,71 / 25 / 106–111 |
| **nasa-neows** | `lib.rs:188–193`, **6**, D; metered helper `228–233`, **6**. Receive; capture quota; decode; policy strips success quota wrapper from returned value. | `lib.rs:153–172`, **20**, retains only kind/status/two quota numbers; provider text and underlying source discarded. B—. | 5 / 31–32 / 44–45 |
| **notion** | **—**. `client.rs:221–243`, **23**: decode successful value+head; otherwise bounded raw bytes, parse provider error and require object/status agreement. | `error.rs:94–130`, **37**. Incomplete query retains response: `client.rs:102–110`, **9**. Cursor-envelope validation `201–219`, **19**. | 101 / 16 / 56 |
| **ntfy** | `error.rs:170–185`, **16**, D. Explicit Request mappings around prepare/receive/decode; non-2xx Api read. Helper `33–52`, **20**, preserves numeric Retry-After and optional bounded document. | `error.rs:124–132`, **9**. Media label ignored; document/read failures collapse to absent document. B—. | 98–104 / 22–23 / 3,42 |
| **open-library** | `policy.rs:17–30`, **14**, D. Receive; bounded JSON Value HTTP document; otherwise decode. | `error.rs:21–28`, **8**; HTTP carrier `60–63`, **4**. Deliberately avoids inventing a shared provider error schema. B—. | 9–11 / 9 / 130–149 |
| **open-meteo** | **—**. `lib.rs:96–126`, **31**: receive; typed bounded non-2xx problem; runtime format/media selection; bounded bytes; JSON/CSV decode and validation. | `error.rs:89–108`, **20**. Duplicate stage wrapper `17–47`, **31**. A JSON `error/reason` model is used on non-2xx; successful forecast validation is separate. | 47–62 / 5,14 / 92 |
| **openai** | `lib.rs:305–313`, **9**, D; checked helper `287–299`, **13**. Receive; bounded media-agnostic HTTP error; successful value+head. | `lib.rs:250–269`, **20**. Failed ResponseObject remains data in finite calls; stream failed/error events are errors; batch output retains per-item `error`, `159`, and response status `163`. | 6,15 / 13–14 / 49–50 |
| **paypal** | `policy.rs:172–279`, **108**, D. Validate origin/request identity; choose replay permission/budget; token per attempt; classify run; separate read and write uncertainty. Detailed trace below. | `error.rs:289–307`, **19**. Uncertainty retains earlier ambiguity even when later attempts refuse. Token service has separate error schema/client, `auth.rs:68–80`, **13**. B— for API 2xx error envelope. | 9–25,46 / 28–31 / 52,64 |
| **pokeapi** | `lib.rs:199–205`, **7**, D; finish `274–288`, **15**. Receive; retain final URL/head; decode; retain same metadata on failure. | `lib.rs:112–123`, **12**. Native Concord status errors remain unread; URL rejection is separate. B—. | 15–21 / 32,142 / 119 |
| **postmark** | `lib.rs:256–286`, **31**, D. Prepare; identify POST; count submitted items; ambiguous receive/decode failures; bounded HTTP error; validate decoded result count. | `lib.rs:159–183`, **25**. Reply machinery `218–250`, **33**. Batch result decoding `105–149`, **45**, distinguishes accepted/rejected items. Single Acceptance still exposes nonzero ErrorCode as data. | 5 / 183–189 / 60–82 |
| **quickbooks-online** | `policy.rs:27–40`, **14**, D. Prepare into parts; GET→read, other→write. Read `43–67`, **25**; write `69–116`, **48**; finish/fault extraction `141–161`, **21**. | `error.rs:339–361`, **23**. Fault detection uses `format.rs:19–67`, **49**, including a codec-source sentinel; policy restores typed Provider error with status 200. | 9–44 / 36–38,186–187 / 37 |
| **rest-countries** | `lib.rs:144–159`, **16**, D. Receive; bounded typed error document plus stage wrapper; otherwise decode. | `error.rs:212–221`, **10**. Duplicate stage wrapper `77–116`, **40**. Successful paging/lookup contract errors are separate. B— for embedded error discriminator. | 49–75 / 81 / 63–67 |
| **salesforce** | **—**. `transport.rs:48–65`, **18**, send with usage or plain send; async native-error conversion. `error.rs:208–236`, **29**, bounded provider-document classification. | `error.rs:20–43`, **24**. API, stream/open/read/incomplete/job errors separate. API failure has decoded/empty/not-document/too-large/unreadable states. | 55–56 / missing / missing |
| **sentry** | `policy.rs:101–275`, **175**, D. Per-attempt retry/admission/observation; final report and lease; bounded provider error or decoded metadata-rich response. Detailed trace below. | `error.rs:69–114`, **46**. A headless write is Uncertain; 5xx stays Provider; rate metadata/pagination/retry-stop errors remain separate. B—. | 30–57 / 43–44 / 43–47 |
| **shopify** | **—**. `context.rs:155–168`, **14**, receive; check served version on successful head; native decode. | `context.rs:217–230`, **14**. `graphql.rs:245–274`, **30**, retains absent/null/present data, errors, cost, throttling and partiality. Mutation userErrors are separate, `501–510`, **10**. | 61–64 / missing / missing |
| **slack** | **—**. `lib.rs:233–277`, **45**: receive; bounded raw HTTP error/429; decode; require ok; convert provider failure; reject conflicting error; retain warnings/metadata. | `error.rs:198–236`, **39**. Envelope `wire.rs:9–18`, **10**. HTTP 200 `ok:false` is API failure; contradictory/missing failure fields are Envelope failures. | 22 / 7–11 / 34 |
| **snowflake** | **—**. `client.rs:181–230`, **50**: receive; bounded raw+parsed HTTP error and handle correlation; accepted-status check; bounded successful decode. | `error.rs:88–114`, **27**. ServerError `30–41`, **12**, retains raw/parsed/read/decode outcomes. Status-conditioned pending/complete codec `api.rs:12–30`, **19**. | 19,33–41 / 40–43 / 32,44 |
| **spotify-web-api** | `session.rs:330–424`, **95**, D. Writes dispatch once under deadline; reads retry one 401 and configured eligible 429s; credentials invalidated by generation. Final helpers `433–456`, **24**. | `error.rs:253–274`, **22**. `after_write`, `326–334`, **9**, wraps deadline or delivered/unknown Concord failure while preserving explicit API refusals. B—. | 19 / 77 / 56 |
| **square-payments** | `lib.rs:102–115`, **14**, D. Receive; bounded typed non-2xx error; otherwise decode. | `error.rs:5–10`, **6**. `outcome_unknown`, `25–37`, **13**, uses POST identity plus delivery/5xx/redirect. Version mismatch is a provider code, not a fixed status. B—. | 10–11 / 77–94 / 47 |
| **strava** | `auth.rs:287–383`, **97**, D; token policy `lib.rs:136–141`, **6**, D; finish `143–169`, **27**. Scope/origin checks; credential and quota per attempt; safe GET retries; no upload replay. | `error.rs:61–82`, **22**. `from_call`, `84–91`, uses delivery. Upload polling turns successful-body `error` into Processing, `upload.rs:150–171`, **22**. | 12–16 / 18–19 / 14,51 |
| **stripe** | **—**. `send.rs:463–498`, **36**: receive; bounded media-agnostic error envelope; classify document failure; otherwise value+projected metadata. | `send.rs:310–328`, **19**. Ordinary generated send remains a second, native result surface; SDK helper is opt-in. B—. | 134–152 / missing / missing |
| **twilio** | **—**. `error.rs:185–204`, **20**: receive; bounded JSON provider exception; reduce document failures to four categories; otherwise value+head. | `error.rs:82–103`, **22**. HTTP-200 failed/undelivered message remains data, `models.rs:131–150`, **20**. | 8–29,63–72 / 46–50 / 21, unchanged |
| **usgs-earthquake-catalog** | `policy.rs:32–88`, **57**, D. 36–69 validate final request bounds; 70 receive; 71–84 accept only 200, normalize 204 and bounded text errors; 85–86 decode+head. | `error.rs:91–109`, **19**. 204/404 no-data interpretation varies between direct lookup, traversal and export. B—. | 14–15 / 81–84 / 68,121–123 |
| **world-bank** | `lib.rs:205–261`, **57**, A. 208 receive; 209–224 bounded error/data tuple discrimination on non-2xx; 226–231 successful error tuple; 232–257 validate/project page. | `lib.rs:119–150`, **32**. A one-element error tuple produces Provider even at HTTP 200; malformed HTTP document becomes Http. | 4–6 / 22–23 / 74–75 |
| **xero-accounting** | `policy.rs:33–124`, **92**, D; token policy `auth.rs:33–38`, **6**, D. Verify tenant header; require grant; credential/admission per attempt; safe-read 429 retry; final bounded provider handling `policy.rs:137–152`, **16**. | `error.rs:87–118`, **32**. Create classification `lib.rs:427–456`, **30**; invoice validation `models.rs:291–294`, **4**, distinguishes explicit rejection from uncertain contract failure. | 29,59 / 112–119 / 174 |
| **zendesk-support** | `policy.rs:163–175`, **13**, D. Operation+prepare, GET→read, other→write. Read `72–96`, **25**; write `98–135`, **38**; retry classifier `140–161`, **22**. | `error.rs:233–255`, **23**. HTTP, update Collision, Uncertain with sent key, deadline and pagination errors distinguished. Ticket envelopes remain caller-visible. | 50–78,107–121 / 161–167 / 18–21 |

The 16 `SendWith` absences are therefore: **ClickHouse, Distribution Registry, GitHub, Google Drive, HubSpot, Kubernetes, Linear, Microsoft Graph, Notion, Open-Meteo, Salesforce, Shopify, Slack, Snowflake, Stripe and Twilio**.

**Detailed traces for the larger `SendWith` implementations**

These account for the substantial policy bodies rather than hiding them under “retry plumbing.”

- **Adyen, `policy.rs:289–462`:** `289–292` establishes `Confirm` and response/error types; `293–305` prepares and restricts destination; `306–314` extracts and validates the idempotency key; `315–323` requires bounded fixed bytes; `324–330` reserves method/URL/header/body identity; `333–360` recreates bytes, checks the reservation before each dispatch, and classifies transport/transient responses; `361–375` preserves deadline evidence; `376–396` separates completed dispatch failures from reservation refusal; `397–406` revokes a lost guarantee; `407–438` reads bounded errors and interprets provider codes 703/704; `439–447` makes unreadable success reconciliation-required; `448–459` confirms response identity; `460` returns value, head and retry report. Header classifiers are `258–287`. Removing this into a general retry policy would risk its retention and identity guarantees.

- **Sentry, `policy.rs:101–275`:** `104–110` validates operation and identifies writes; `111–124` prepares budget; `125–143` acquires both admissions per attempt and returns the lease; `144–190` observes quota and retry metadata, retries reads on selected failures and writes only on 429; `196–208` handles refusal/headless write; `210–232` distinguishes answered writes from deadline/replay stops; `233–246` keeps the final lease during bounded provider-document consumption; `247–251` refuses invalid scheduling metadata; `253–270` validates next cursor and returns value/head/rates/report; `272` releases the lease.

- **PayPal, `policy.rs:172–279`:** `176–188` captures operation, validates origin, extracts request ID and grants replay only where supported; `190–219` selects read/write budgets and acquires a token per attempt; `220–222` retains aggregate ambiguity; `224–238` returns read results/refusals; `240–247` constructs write uncertainty context; `248–258` preserves cancelled dispatch and later-token refusal; `259–276` distinguishes no response, unreadable success, server failure and later explicit refusal. Earlier ambiguity remains consequential.

- **Spotify, `session.rs:330–424`:** `334–362` handles writes once, including token acquisition, 401 invalidation, decoding within deadline and post-dispatch uncertainty; `363–370` sets read retry limits; `371–395` recreates request and applies current credentials, identifying 401 versus 429; `396–413` permits one unauthorized replay and bounded eligible throttling; `419–422` finishes the retained final response/document.

- **Strava session, `auth.rs:287–383`:** `290–306` checks operation-required scope; `307–314` prepares, verifies origin and creates a read-only replay template; `315–326` gives reads two attempts and writes one; `328–344` acquires credentials and the applicable quota keys; `345–360` dispatches, invalidates rejected generation and observes quotas; `364–374` retries only selected read 502–504 or headless transport failures; `378–381` finalizes with write-ambiguity context and attempt count. TokenPolicy’s six lines deliberately bypass that application-session behavior.

- **Xero, `policy.rs:33–124`:** `36–45` binds the policy to the tenant header; `46–60` checks grants and read/write replay budget; `61–85` obtains token and both admissions, dispatches, invalidates 401 and observes 429; `86–102` releases a retry response’s lease while retaining the final one; `105–113` authorizes only valid read-429 delay retries; `119–122` keeps the final lease through finishing. Create uncertainty and invoice validation are subsequently handled by the public operation wrapper.

The other implementation blocks are fully enumerated in the table, including S3’s four output specializations and both Cloudflare projections.

**Distinct mechanics and smallest candidate changes**

The “savings” column deliberately separates observed removable fragments from unproved whole-SDK savings.

| Mechanic | Smallest change and location | SDKs / line effect | Public API effect and counterexamples |
|---|---|---|---|
| Repeated `Metadata/Read/Decode` wrapper enum | **Docs only:** recommend storing `concord::Error` and matching `kind()`. SDK migration optional. | **4 SDKs; 148-line blocks → four aliases: 144 lines deleted**, excluding caller/formatting migration. | Removes SDK enum variants and Datadog’s forwarding `concord()` method. Existing consumers match those variants: Frankfurter `tests/first_use.rs:349–376`, REST Countries `tests/protocol.rs:490–492`, Open-Meteo `tests/protocol.rs:187–234`. |
| Owned raw bytes beside metadata | **Docs only:** demonstrate `document::<Binary, Bytes>`. | **3 direct substitutions, 3 lines**; broader applicability to Airtable, Dropbox, Slack and Snowflake has no established net saving. | Adds nothing. `Binary` must be available through `concord-codecs` or the facade’s `extras` feature. Does not replace strict JSON document checks. |
| Ordinary non-2xx typed-document branch | Optional **core** helper returning a safely formatted `HttpDocumentError<D>` while leaving SDK classification outside. | Five small witnesses—Anthropic, DigitalOcean, Intercom, Open Library, Square—contain **32 lines of status branches**. Net deletion is **unproved and necessarily smaller** because conversion remains. | Would add one carrier and one consuming method. Weak fit for Airtable/Dropbox/raw retention, USGS’s exact-200 rule, S3 304, Elasticsearch 404/409 and every 2xx-error envelope. I would retain the existing primitives until a prototype shows worthwhile net reduction. |
| Provider document schema and status classification | **Leave to SDKs.** | **41 document-using SDKs; 0 proposed removal.** | No shared provider error enum. “401”, quota codes, missing resources and optimistic concurrency mean different things across domains. |
| Whole-result failure inside 2xx | **Docs only:** teach explicit discriminators and delayed payload decoding. | Eight clear families: arXiv, Cloudflare, Kraken, MediaWiki, QuickBooks, Slack, World Bank, Xero. **0 semantic lines removable.** | No universal envelope API. Cloudflare’s 54-line two-phase decoder is required to avoid decoding an irrelevant failure payload. MediaWiki warnings and Kraken error lists follow different rules. |
| Partial data and completeness | **Leave to SDKs; docs distinguish HTTP completion from usable completeness.** | Linear, Shopify, Datadog, Drive, Elasticsearch, Notion: **6 SDKs; 0 proposed removal.** | An automatic error promotion would discard useful partial data or reject accepted diagnostics. An automatic success projection would hide incomplete search/query results. |
| Per-item results and attribution | **Leave to SDKs.** | Elasticsearch, HubSpot, Postmark, OpenAI: **4 SDKs; 0 proposed removal.** Concrete correlation witnesses: ES **51 lines**, HubSpot **45**, Postmark batch conversion **45** plus Reply **33**. | Position, ID, action, missing result and batch-level uncertainty differ. A generic `Vec<Result<T,E>>` cannot enforce those correspondences by itself. |
| Stream-embedded provider errors | **Existing codecs for framing; SDK interpretation.** | Anthropic, ClickHouse, Kubernetes, OpenAI: **4 SDKs; 0 proposed semantic removal.** | Anthropic errors terminate with partial message; Kubernetes errors are typed watch events; ClickHouse failure may follow delivered rows. Folding these into finite `SendWith` would weaken streaming guarantees. |
| Retry/auth/admission composition | **Existing `concord-retry`, `concord-auth`, `concord-ratelimit`; docs first.** | Six large impls above total **741 lines**, but these are not a shared removable block. **0 demonstrated removal.** | Current `Retry::receive_with` already has async fallible per-attempt hooks, independent refusal and completed/no-response accounting: `concord-retry/src/concord.rs:107–130,202–275`. Earlier requests for that capability are historical. |
| Signing | **Leave algorithms to SDKs; retain core decomposition.** | S3 and Kraken: **2 SDKs; 0 proposed removal.** | S3 signs an already built request; Kraken adds nonce to exact form bytes. Generic signing mutation would add an abstraction without eliminating these algorithms. |
| Client-wide policy dispatch | Consider **macros**, nearest inherited scope/endpoint `send` using existing `SendWith`. | Direct structural witnesses in **7 SDKs**: B2, Datadog, Elasticsearch, PayPal, Postmark, QuickBooks, Xero. **No established net line saving**; identity-dispatch fragments are roughly tens of lines, and declarations would grow. | Adds policy placement/override syntax. Preserve client → scope → endpoint. QuickBooks can use `company(connection)`; PayPal can separate token behavior. Counterexamples: global S3 signer, Discord limiter and shared sessions benefit from one client policy. |
| Opaque associated result | **Docs/adoption first** for five SDKs; consider endpoint-specific result declarations only for remaining three. | **5 SDKs can improve signatures with 0 runtime lines added or removed**. Generic inner-type cases remain **3 SDKs**. | No API addition needed for Airtable, S3, Frankfurter, Nager, World Bank. A pattern-matching result-template language would add syntax and maintenance to serve Cloudflare/Kraken/MediaWiki; not justified by line reduction alone. |
| Prepared/followed request bypass | **Docs and SDK finish helpers.** | Visible in DigitalOcean, GitLab, Mailgun, Zendesk, PokeAPI and others. **0 universally removable lines.** | `Prepared<T>` lacks the policy and static endpoint interface required by `SendWith`. Adding policy-carrying prepared handles is a larger API. Native inspection and deliberately unsigned preparation are useful counterexamples. |
| Public SDK error matching | **SDK-owned named errors and documented accessors.** | All 55 expose native or SDK errors. **0 universal removal.** | A common outer enum would either lose domain distinctions or gain many generic escape variants. Preserve typed provider documents, authoritative status and explicit uncertainty. |

Two implementation details materially constrain the candidate policy-placement change:

- The parser currently rejects `send` outside the client at `syntax.rs:993–999`; emission reads the root client’s policy at `emit.rs:1574–1612`.
- A policy value is also automatically treated as sensitive, including its placements, at `model.rs:572–575`. QuickBooks uses one `Connection` both as policy and realm path value. Moving policy ownership to a scope must preserve that coupling and deliberately reconsider the incidental realm redaction.

**Provider errors are repeated in shape, not interchangeable in content**

The repeated carrier is commonly:

```rust
meta: ResponseMeta,
document: Result<ProviderDocument, concord::Error>,
```

Examples include Anthropic `error.rs:231–234`, DigitalOcean `42–48`, Elasticsearch `293–296`, Intercom `106–111`, Open Library `60–63`, Square `75–81`, and Mailgun `82–87`.

Their surrounding behavior differs:

- **Retain document failure:** these carriers preserve why a document was unavailable.
- **Collapse to optional document:** Mastodon `error.rs:31–42`, ntfy `33–52`, Postmark `lib.rs:276–280`, OpenAI `lib.rs:289–295`, Kubernetes `error.rs:52–62`.
- **Retain raw and parsed bodies together:** Airtable, Dropbox, Linear, Slack and Snowflake.
- **Retain malformed bytes only:** GitLab and Distribution Registry use `Error::body_unredacted()` after document decoding fails.
- **Reduce failures to a small public category:** Twilio and Stripe.
- **Require agreement between body and HTTP status:** Notion and B2.
- **Use HTTP status as authoritative despite body disagreement:** Frankfurter and DigitalOcean.

A shared carrier could safely hide `D` in `Debug`; it cannot decide these retention and interpretation policies. Replacing the tuple returned by `document` would also break **41 SDKs**. An additive carrier should therefore earn its place through measured deletion rather than changing that return type immediately.

**Uncertainty and caller matching**

`Error::delivery()` supplies evidence, not a replay decision:

- `NotSent`: build/encoding refusal.
- `Unknown`: transport failure without a response head.
- `Answered`: a response head arrived, including unreadable bodies.

That is implemented in [core error.rs:347–358](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/integration/crates/concord-core/src/error.rs:347). A refused connection remains `Unknown`; redirect or nested streamed-body failures prevent treating `is_connect()` as proof of non-delivery.

The SDK rules are observably different:

- **Elasticsearch:** unknown delivery, unreadable successful response, provider 5xx and contradictory response correlation can be uncertain; a known PartialBulk report is not uncertain (`error.rs:53–74`).
- **Square:** only write operations qualify; provider redirects and 5xx also count (`error.rs:19–43`).
- **HubSpot:** unsent and most explicit 4xx failures are definite; 408 and other unresolved outcomes become CreateUncertain (`lib.rs:575–587`).
- **B2:** upload source progress and delete status affect uncertainty (`upload.rs:774–787`, `storage.rs:237–249`).
- **Spotify:** dispatched deadline/native failure gets UncertainWrite, while explicit API refusal remains visible (`error.rs:325–334`).
- **Sentry:** headless writes become Uncertain, but 5xx remains Provider with instructions to re-read (`policy.rs:199–208`; `MAINTENANCE.md:43–47`).
- **QuickBooks/Zendesk/PayPal:** uncertain writes retain provider-specific reconciliation identity—realm/request ID, idempotency key, or PayPal request ID.

Callers consequently use three distinct public interfaces:

1. **Outer variant matching**, such as Provider/Http versus Concord versus Uncertain.
2. **Kind/accessor matching**, such as Drive `kind()/provider()/document_problem()`, Xero `kind()/write_uncertain()`, or Square `outcome_unknown()`.
3. **Result-content matching**, such as Postmark `BatchResult`, Kubernetes `WatchEvent::Error`, Shopify `data/errors`, or Twilio message status.

Those distinctions should remain visible. A single Concord “retryable” or “provider error” flag would not represent them adequately.

One current inconsistency deserves review: QuickBooks `Error::status()` delegates a Concord error to its **Status-only** `status()` accessor (`error.rs:372`), whereas successful-body decode failures have their HTTP status in `head()`. This is a source-level observation; its intended accessor contract needs deciding before calling it a defect.

**What Concord could remove or shrink**

- **Deprecate the narrow `Error::meta()` accessor in favor of `head_meta()`.** Its documented behavior only covers retained Status/Metadata responses, while `head_meta()` covers all failures after a head. I found **no direct production use of Concord’s `Error::meta()`** in these SDKs; similarly named calls resolved to SDK types. The definition/documentation occupies `concord-core/src/error.rs:268–276`, **9 lines**. This would remove one easily confused accessor, with zero SDK migration identified here. External users are unknown.

- **Keep `Error::status()` semantics explicit.** It is used to distinguish Status errors in Kubernetes `error.rs:35–40`, Nager `error.rs:38`, and HubSpot. Silently broadening it would change those tests of failure stage. A future rename/deprecation is a compatibility decision, not a free simplification.

- **Avoid adding `bytes_limited_with_meta`.** The existing Binary/document composition supplies the capability. Add a recipe instead.

- **Avoid adding a second general error-stage enum by default.** `DocumentError::{Metadata,Read,Decode}` would centralize four copies but duplicate `ErrorKind`, merge Limit with Transport again, and add a public type. The existing error is sufficient unless preserving the SDK enum API is a firm compatibility requirement.

- **Reduce repetitive bypass documentation, without removing the bypass.** Generated `send` already explains policy bypass, and generated `prepare` already says it precedes signing/decorating (`emit.rs:1628–1632,1685–1688`). Several notes request behavior already present. Naming the actual policy and the two error types more explicitly is a small documentation improvement.

- **Consider suppressing unused generated terminal methods only with concrete demand.** B2 hides generated upload/download sends because their native targets and handling are inappropriate (`FRICTION.md:48–59`). A narrow suppression feature could remove misleading generated items, but produces **no SDK source-line saving** and adds syntax. `send none` must distinguish “use native Concord send” from “generate no send method.”

- **Retain `Sends`.** It has production uses in **7 SDKs**: arXiv, Cloudflare, DigitalOcean, FRED, NASA, REST Countries and Spotify. It preserves generic sending through the SDK policy; it is not redundant with `Endpoint`.

- **Do not simplify `SendWith` merely because every current output is a `Result`.** Replacing arbitrary `Output` with associated success/error types would add associated declarations and migrate all implementations. No source reduction is demonstrated, and envelope projections still need flexible success types.

**Historical notes versus current source**

Several reports preserve earlier limitations:

- Twilio’s friction describes a manual bounded-read sequence; current `error.rs:188–195` uses `Received::document`.
- Stripe’s friction describes older native error-document plumbing; current `send.rs:483–492` uses `document`.
- NASA and FRED discuss needing a policy for success-body limits; current declarations use `limit`.
- Postmark and Zendesk first-use notes mention opaque associated results; current declarations have explicit result templates.
- Requests for async fallible retry admission are addressed by current `receive_with`.
- Requests for policy-bypass prose are substantially addressed in current macro emission.

These remain useful provenance, but should not be counted as unresolved framework gaps.

**What remains undecided**

- **Net savings from a shared HTTP-document carrier or policy helper.** The repeated fragments are measured above; no replacement was implemented, formatted or compiled. Whole error-file sizes are not credible savings estimates.
- **Scope/endpoint policy inheritance.** Evidence supports expressiveness gains, especially QuickBooks and PayPal. It does not settle override syntax, constructor/default behavior, result documentation, or sensitivity propagation.
- **Compatibility costs.** Removing duplicate SDK enums would affect exercised public matches. The wider external usage of Concord accessors and arbitrary `SendWith::Output` is outside this 55-SDK evidence set.
- **Uniform body retention.** Some SDKs intentionally discard raw/provider content; others promise it for reconciliation. This is an unresolved API-policy choice, not missing generic byte machinery.
- **Provider correctness beyond the worktree.** Existing test and document claims were read, not rerun or externally verified.