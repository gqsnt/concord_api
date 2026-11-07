Read-only sweep of all **55 SDKs**, at commit `af6840c97a316493f65b266f13461adde4585a3e`. No builds, tests, SDK execution, external services, or file changes.

**The strongest finding is that Concord needs less new policy machinery than the older friction notes suggest.** `receive_with` already addresses async credentials, fallible admission, and retry-state lifetimes. The remaining codec gaps are separate problems: streamed multipart, exact-length one-shot bodies, XML parsing, bounded CSV spooling, and nested forms. Combining them into one transport or capability abstraction would obscure their different guarantees.

**Adoption observed in manifests and SDK source**

| Capability | SDK adoption | What that count means |
|---|---:|---|
| `concord-auth` | **20/55** | Basic: 9; Bearer: 15; other scheme: 1; `TokenCache`: 7. These overlap. |
| `concord-retry` | **16/55** | Attempt engine: 9; `Retry-After` parser: 15; parser without engine: 7. |
| `Retry::run` | **3** | Adyen, Spotify, Xero. |
| `Retry::run_tokio` | **2** | Backblaze B2, Strava. |
| `Retry::receive` | **2** | Cloudflare, Zendesk. |
| `Retry::receive_with` | **2** | PayPal, Sentry. |
| `concord-ratelimit` | **4/55** | arXiv, Sentry, Strava, Xero. Discord implements its own limiter. |
| `concord-codecs` | **55/55**, directly or through facade | Five SDKs depend on it directly. |
| JSON feature | **53 enabled; 52 use `Json`** | ClickHouse enables default JSON but uses other codecs. S3 and arXiv do not enable JSON. |
| Form | **8** | HubSpot, Kraken, PayPal, QuickBooks, Spotify, Strava, Twilio, Xero. |
| Buffered multipart | **3** | Drive, Mailgun, OpenAI. |
| SSE | **4** | Anthropic, Mastodon, ntfy, OpenAI. |
| JSONL family | **6 enabled; 5 use shared implementation** | ClickHouse, Elasticsearch, Kubernetes, ntfy, Shopify use it; OpenAI writes/parses batch JSONL itself. |
| CSV | **4** | ClickHouse, Open-Meteo, Salesforce, USGS. |
| Handwritten XML frontends | **4** | S3, arXiv, Europe PMC, FRED. |
| Production HTTP-client builder | **25** | All disable redirects; 22 also explicitly disable native retries. |
| Same-origin redirect policy | **2** | GitHub and GitLab, separately from those 25 builders. |

Feature counts describe manifests, not a built, feature-unified dependency graph.

**Most valuable mechanics and candidate changes**

**1. Async prepared retries: adopt the existing adapter before adding another API.**

Observed:

- Adyen decomposes the prepared request because its replay reservation can expire during backoff: `examples/adyen-checkout-sdk/src/policy.rs:331–360`, **30 lines**.
- Xero manually clones, authenticates, leases, admits, executes, and conditionally releases the lease: `examples/xero-accounting-sdk/src/policy.rs:61–104`, **44 lines**.
- Strava manually handles first-use versus cloned requests, authentication, admission and dispatch: `examples/strava-sdk/src/auth.rs:327–363`, **37 lines**.
- The current adapter already accepts an async fallible hook and preserves separate outcome, context, refusal and dispatch accounting: [concord.rs:202](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/integration/crates/concord-retry/src/concord.rs:202). PayPal and Sentry already use it.
- Xero’s older request for an async adapter appears in `FRICTION.md:25–43`; Adyen’s appears at `FRICTION.md:7–21`.

Inference and smallest change:

- **Location:** docs and SDK migrations; **no new public API**.
- Strong migration candidates: **Adyen and Xero**. Their cited blocks contain **74 lines**, but most classification and admission remain. Estimated net removal: **15–30 lines combined**, not 74.
- Strava is a third candidate, with smaller and less certain savings because its one-shot and write-error mappings must survive.
- Do **not** remove `run`. Spotify consumes a bounded 429 document to distinguish quota exhaustion from temporary throttling; `receive_with` deliberately classifies a borrowed unread outcome. Its `Attempt<T>` and body classification are at `session.rs:320–328,371–421`.
- Backblaze’s factory also obtains/replaces upload targets and returns provider attempt evidence, rather than simply executing one fixed prepared request: `upload.rs:643–709`.

The useful documentation change is a short selection rule: **`receive` for synchronous decoration; `receive_with` for async/fallible decoration and retained attempt state; `run` when the attempt must produce an SDK-owned outcome or consume classification documents.**

**2. Refresh-token rotation is not one missing transaction callback.**

The seven cache adopters still own materially different acquisition protocols:

| SDK | Remaining handwritten mechanic | Current footprint |
|---|---|---:|
| Registry | Challenge parsing, trusted realm, access scope, token document and one challenge replay | `client.rs:147–278`: **132 lines** |
| HubSpot | Installation secrets, uncertainty latch, optional rotation, refusal classification | `lib.rs:376–388,447–527`: **94 lines** |
| PayPal | Client-credentials acquisition; no rotating refresh token | `auth.rs:172–204`: **33 lines** |
| QuickBooks | Realm binding, seed, replacement epoch, remembered rejected grant, latest rotation | `auth.rs:519–583,662–746`: **150 lines** |
| Spotify | Seed, absent/rejected refresh state, optional rotation, concurrent replacement protection | `session.rs:118–194`: **77 lines** |
| Strava | Stored provider token document, mandatory replacement, preserved scope, validation ordering | `auth.rs:182–222`: **41 lines** |
| Xero | Durable pre-exchange marker, process lock, staged pair, fsync/rename, recovery | `store.rs:1–343`: **343 lines**, plus acquisition `auth.rs:91–154`: **64** |

These are gross footprints, including provider policy and documentation.

Xero already commits the rotated pair **inside the fallible acquisition closure before returning `Token`**: `auth.rs:134–145`. Its notes explicitly recognize that no callback is missing: `FRICTION.md:7–13`. HubSpot’s proposed commit hook, `FRICTION.md:8–15`, would not supply Xero’s crash protocol.

- **Recommendation:** retain `TokenCache`; add a rotating-credential acquisition recipe to **auth docs**.
- **Public API:** none.
- **SDK source removed:** **0** from a recipe; it prevents unnecessary framework growth.
- **Counterexamples to a universal rotating-token abstraction:** PayPal has client credentials; registry has realm challenges; Spotify replacement is optional; Strava preserves a replacement before rejecting an unusable access token; Xero requires durable fail-closed state.

A separate, smaller candidate is **seeding an already-issued access token**. QuickBooks and Spotify both store a seed and calculate remaining lifetime themselves; Spotify records the clock duplication in `FRICTION.md:50–66`.

- Possible API: `TokenCache::insert(key, Token) -> Result<Snapshot, RefreshError<E>>`.
- **Location:** existing auth crate.
- **Reach:** two clear SDKs, Strava potentially.
- **Estimated removal:** **8–15 SDK lines** across the two clear cases.
- It must preserve capacity checks, checked lifetime arithmetic and generation rules when replacing an in-flight acquisition. The notes’ infallible `insert` sketch omits those refusals.
- Given that small saving, this is lower priority than documenting the existing acquisition boundary.

**A correctness concern from inspection:** HubSpot captures `inner` in the future stored by `inner.cache` (`lib.rs:447–452`); Strava does the equivalent (`auth.rs:182–186`). The cache retains paused acquisition futures, `cache.rs:261–284`, and its documentation explicitly warns against this ownership cycle, `README.md:56–63`.

I infer that abandoning all callers during acquisition can retain the cache and its owner. This was **not experimentally verified**. Fixing these **two SDKs** means capturing acquisition inputs separately, as Xero, QuickBooks and Spotify do. It requires **no Concord API** and is not a line-removal opportunity.

**3. Transport recipes repeat, but they do not express one universal transport contract.**

Observed production builders:

- **22** repeat both `.redirect(Policy::none())` and `.retry(retry::never())`.
- S3, ClickHouse and Graph disable redirects without explicitly disabling native retries.
- S3 additionally disables decompression: `client.rs:32–39`.
- Snowflake disables decompression so it can bound and validate gzip itself: `client.rs:124–134`, `compression.rs:51–69`.
- GitHub and GitLab allow same-origin redirects and stop cross-origin redirects: `context.rs:71–85` and `lib.rs:84–98`.
- Several SDKs deliberately accept an opaque caller client; construction settings cannot be recovered from it.

Smallest changes:

| Candidate | Placement/API | Reach and estimated removal | Counterexamples |
|---|---|---|---|
| Consolidate transport recipes and explain their distinct guarantees | **Docs only**, no API | All SDK authors; **0 source lines** | Recipes still need provider choices. |
| Optional common builder disabling redirects and native retries | Facade, e.g. one `single_dispatch_client_builder()` function | **22 SDKs; about 44 lines** replacing three builder expressions with one | GitHub/GitLab redirect behavior; three builders currently omit retry suppression; provider timeouts, proxies, TLS and decompression remain. |
| A strict-client wrapper or automatic core transport mutation | Core | No demonstrated necessary saving | Cannot certify arbitrary supplied clients; would expand every endpoint’s transport contract. Reject. |

A helper does **not** belong in `concord-auth`: that crate currently has no reqwest dependency (`Cargo.toml:16–19`). Keeping transport construction separate preserves that useful boundary.

**4. Streamed multipart is the clearest repeated codec gap.**

- Mailgun uses shared multipart for bytes, then a separate reqwest encoder for a streamed attachment: `message.rs:312–342`; the streamed branch is **23 lines**.
- Strava builds and wraps a reqwest form stream: `upload.rs:85–101`, **17 lines**.
- Both retain one-shot ownership and bind the runtime boundary through `EncodedBody`.
- Drive already uses shared related multipart: `upload.rs:91–101`. Its older manual-framing account in `FRICTION.md:58–71` no longer describes that implementation.

Candidate:

- **Location:** `concord-codecs` multipart feature.
- Add an explicit streamed-part/body route; keep the existing buffered route.
- **Reach:** **2 SDKs**, roughly **20–30 net lines** removable from approximately 40 lines of encoder adaptation. Provider field mapping, file validation and request types remain.
- Do not silently buffer or promise the buffered encoder’s “boundary absent from every part” guarantee for an unread stream.
- **Counterexamples:** Drive and OpenAI already fit buffered multipart; converting their bodies to streams would discard replayability and add ownership complexity.

**5. Exact-length one-shot upload and bounded CSV spooling are different mechanics.**

Backblaze implements a body with an exact `SizeHint`, early-source failure handling and a byte/failure tally: `upload.rs:25–146`, **122 lines**. Its provider requires length and refuses chunked transfer.

- Possible codec API: an exact-length stream-to-body adapter, with explicit short-source behavior.
- **Reach:** **1 SDK**.
- **Estimated removal:** **40–70 lines**; its evidence tally and upload classification cannot simply disappear.
- **Counterexamples:** Mailgun/Strava streaming multipart has unknown framing length; ordinary record uploads intentionally stream without a known total. Nothing should fabricate a size hint.
- **Recommendation:** keep SDK-owned for now; improve the exact-body recipe before adding an API from one witness.

Salesforce instead consumes rows into a **bounded complete byte buffer before sending**. Its spool methods and state are `csv.rs:247–341`, **95 lines**. It also separately constructs header-only and headerless writers at `217–237`.

- Candidate: `Upload::collect_bounded(max)` in the existing **codecs stream module**, or a bounded CSV writer sink.
- **Reach:** **1 clear SDK**; estimated **40–60 lines** removable, while retaining row counts and public `SpooledCsv` behavior.
- **Counterexample:** ClickHouse’s lazy inserts should remain lazy; automatically collecting them would change memory and failure timing.
- Salesforce’s `text/csv` upload media choice (`api.rs:153`) is separate again from CSV record encoding. Do not combine media override, bounded collection and known-length streaming into one option.

**6. XML has four adopters for a small frontend, not four witnesses for universal XML-to-model decoding.**

Repeated parser setup:

- arXiv: `atom.rs:25–34`, **10 lines**.
- Europe PMC: `xml.rs:90–98`, **9 lines**.
- FRED: `codecs.rs:19–36`, **18 lines**, including its byte check.
- S3: `xml.rs:168–179`, **12 lines**, including positioned diagnostics.

Candidate:

- **Location:** optional XML support in **codecs**.
- A bounded UTF-8, namespace-aware parsing helper with explicit node/DTD policy and safe diagnostics.
- **Reach:** **4 SDKs**, approximately **25–35 net lines** removed from **49 lines** of setup.
- No core or macro syntax.
- **Counterexamples:** FRED maps attributes into JSON-shaped models; arXiv matches namespaces and error entries; Europe PMC validates `responseWrapper`; S3 interprets listing/error roots and URL-encoded fields. Those projections remain SDK code.
- Exposing a DOM type would also expose a parser dependency in the public API. That tradeoff remains undecided.

Shared media checking is **already public**, `concord-codecs/src/media.rs:14–71`, and is used by arXiv, Europe PMC, FRED and Zendesk. FRED’s original “helper is private” friction is explicitly superseded at `FRICTION.md:55–67`.

S3’s manual media checker, `xml.rs:28–50` (**23 lines**), could become a short call to it without a new API. However, validating parameter syntax would tighten S3’s currently permissive semicolon-splitting behavior.

**7. Rate-limit non-adoption mostly reflects missing provider policy, not missing scheduling machinery.**

The four adopters exercise different parts:

| SDK | Shared mechanics used | Provider work retained |
|---|---|---|
| arXiv | Fixed budget, concurrency lease, wait mode | One request per three seconds; lease through body processing (`policy.rs:21–36,62–76`). |
| Sentry | Budget, concurrency, observations, cooldowns, wait mode | Endpoint buckets, header parsing, changing concurrency, write/read classification (`policy.rs:63–82,125–190`; `rate.rs:49–100`). |
| Strava | Atomic admission across four keys, window/global observations, refuse mode | Header pairs, quarter-hour/day resets, upload exemptions, final 429 policy (`rate.rs:49–102`; `auth.rs:339–360`). |
| Xero | Concurrency lease, cooldown, refuse mode | Five-call cap and bounded GET retry; no invented distributed quota (`policy.rs:25–31,70–101`). |

Discord implements learned route/bucket aliases, resource identity, windows and a global gate in `limiter.rs:165–338`, **174 physical lines**. Its admission deliberately does not decrement a locally predicted count, and it does not queue or sleep (`limiter.rs:194–208,233–258`).

- **Smallest change:** no crate change; an opt-in Discord migration could reuse observations and refusal.
- **Reach:** **1 SDK**; approximately **40–60 generic code lines**, not the whole limiter. Route aliases, fractional parsing, body/header precedence and typed gate errors remain.
- **Counterexamples:** changing Discord to charged admissions or global FIFO queues changes behavior. Intercom’s missing reset must stay unknown (`FRICTION.md:11–13`; `MAINTENANCE.md:170`). Shopify’s cost-based GraphQL observations are not request-count windows. NASA exposes quota observations without predicting reset.
- **Recommendation:** retain the optional crate; do not merge it into retry or make it a default send layer.

**8. Header helpers and delay parsing have smaller adoption opportunities, with semantic differences.**

- Seven SDKs express Bearer directly in declarations: Airtable, Dropbox, GitHub, Drive, Notion, Salesforce and Slack. A one-line inherited header is already cheap; replacing it with a constructor plus configuration may **add code**.
- Other SDKs construct headers manually. Graph applies a narrower bearer alphabet (`client.rs:137–148`); Kubernetes permits a broader header-valid token (`ids.rs:237–245`). Replacing validation wholesale with `bearer()` would change their contracts.
- Datadog accepts header-valid keys after rejecting blank values (`credentials.rs:62–72`); GitLab and Shopify require visible non-space ASCII. One opaque-key validator would not preserve all three rules.
- Basic adopters retain valid provider checks: Twilio rejects empty credentials (`auth.rs:19–29`), while shared Basic deliberately permits empty fields.

**Recommendation:** docs and selective use of existing helpers, **no new credential framework or raw-key API** justified by the combined evidence. Safe source savings are small and require retaining provider validation.

Delay parsing is also already split appropriately:

- **15 SDKs** use the shared seconds/date `Retry-After` parser.
- Dropbox uses `field::unsigned`, `error.rs:29–35`; GitHub and GitLab use it through numeric projections, `meta.rs:90,101` and `meta.rs:81,99`; Postmark uses it at `lib.rs:272–274`.
- Slack still writes a **19-line** seconds-only parser, `error.rs:37–55`.

Slack could use the existing field helper with approximately **10–12 fewer lines**, but that helper accepts surrounding OWS, unlike Slack’s current direct digit check. Using the retry parser would additionally require deliberately rejecting dates to preserve the public seconds-only contract. Discord’s fractional seconds are another distinct grammar.

**9. Shared stream framing should stay separate from provider completion and envelopes.**

All four SSE SDKs use shared framing. They still need provider event interpretation and completion rules:

- Anthropic: only `message_stop` proves completion; `stream.rs:38–65` adapts framing, while later code assembles the provider flow.
- OpenAI: event completion/failure and batch correlation remain SDK behavior.
- Mastodon: streaming event interpretation and closure semantics.
- ntfy: SSE versus NDJSON payloads plus truncation metadata.

A universal provider-event mapper would not remove those guarantees.

Two narrower findings:

- Anthropic’s owned transport-error recovery takes **22 lines**, `stream.rs:438–459`. Mastodon and ntfy also downcast borrowed errors at `streaming.rs:483–488` and `events.rs:433–440`.
  - Candidate **codecs** APIs: borrowed and owned Concord-error accessors on `ReadError`.
  - **3 SDKs**, estimated **10–18 net lines** saved; no new error framework.
  - ClickHouse’s `SourceError` downcasts are a related but different layer and should not be counted as removed by a `ReadError` method.
- Configured readers already expose `read_for`, including JSONL `lines.rs:500` and SSE `reader.rs:185`. ntfy’s older claim that its wrapper cannot select reader bounds (`FRICTION.md:70–82`) needs a current recipe before another public “open checked” API is added. Changing its source type may add a few SDK lines; it is not a demonstrated deletion.

**10. Nested forms, recoverable JSONL, compression and archives should not be merged into ordinary codecs by default.**

- Stripe’s generic serializer occupies `form.rs:184–728`, **545 lines**, beyond its patch types and error definitions. It implements bracket nesting, omission/clearing and deterministic serialization.
  - Moving a bracket-form serializer into an optional codec could remove hundreds of SDK lines, but **only one SDK** supplies evidence.
  - The eight ordinary Form adopters gain nothing; broadening existing `Form` would change its flat contract.
  - **Recommendation:** leave Stripe’s format in the SDK unless another matching consumer appears. No public Concord addition now.
- OpenAI manually writes and parses batch JSONL at `lib.rs:174–205`, **32 lines**.
  - Existing `JsonLinesWriter::to_vec` can replace writing mechanics, saving roughly **2–5 lines**.
  - Its parser retains per-line failures and detects duplicate IDs. Replacing it with finite `Decode<Vec<T>>` would lose recovery; duplicate correlation is provider policy.
- FRED’s ZIP decoding is `codecs.rs:146–221`, **76 lines**; Snowflake’s partition codec/gzip decoding is `compression.rs:17–69`, **53 lines**.
  - **Location:** SDKs or a separate archive/decompression utility, not core.
  - **New Concord API recommended:** none; **0 removal claimed**.
  - Compressed-byte limits, expanded-byte limits, ZIP member selection and gzip trailer validation are different concerns from JSON/CSV decoding.

**Per-SDK evidence table**

Every SDK participates in this theme through codecs. In the table:

- Paths are relative to `examples/<SDK>-sdk/src/`, unless marked `README`.
- Numbers in parentheses are **inclusive physical lines in the cited block**, including comments/blank lines. They are current footprints, **not all removable boilerplate**; overlapping blocks must not be summed.
- `A—`, `R—`, `L—` mean the corresponding capability crate is absent. Manual equivalents are stated explicitly.
- `F`, `U`, `M` mean `FRICTION.md`, `FIRST_USE.md`, `MAINTENANCE.md`; numbers are relevant lines. `—` means no substantive discussion of the listed capability mechanics found; `∅` means the file is absent.
- A README transport recipe has **zero production-source cost**.

| SDK | Auth, retry and admission evidence | Codec evidence/cost | Transport evidence/cost | Notes mentioning theme: F / U / M |
|---|---|---|---|---|
| Adyen Checkout | A—; raw key `lib.rs:31,84–88` (6). `run`, `policy.rs:292–461` (170). L—. | JSON `lib.rs:35` (1). | `lib.rs:89–94` (6), no redirects/retries, timeout. | F7,68 / U30,174 / M75 |
| Airtable | A—; Bearer template `lib.rs:187` (1). R—, L—. | JSON `lib.rs:188` (1). | README11–13; caller recipe. | F6 response plumbing / U66,79 / M— |
| Amazon S3 | A—; SigV4 implementation `sigv4.rs:1–324` (324). R—, L—. | XML `xml.rs:1–328` (328); Binary request `api.rs:190`; custom head/body formats `object.rs:136,156,388`. | `client.rs:32–39` (8), no redirects/decompression. | F44,147 / U29,70 / M14–15 |
| Anthropic Messages | A—; raw key `lib.rs:79` (1). Retry parser `error.rs:293–296` (4); no engine. L—. | JSON `lib.rs:85`; SSE adapter `stream.rs:38–65` (28). | `lib.rs:175–180` (6). | F55,66,94 / U20,24 / M69 |
| arXiv | A—, R—. Limiter setup `policy.rs:21–36` (16), admission `62–70` (9). | Atom/XML `atom.rs:1–163` (163). | `lib.rs:83–91` (9), timeout, UA, no proxy/redirect/retry. | F9,20–35 / U87 / M189 |
| Backblaze B2 Native | Basic `model.rs:25–32` (8). `run_tokio`, `upload.rs:643–709` (67). L—. | JSON/Binary/Encoded `wire.rs:23,55,71`; sized stream `upload.rs:25–146` (122). | `lib.rs:48–52` (5). | F73–93 / U14,45 / M54–55 |
| ClickHouse | Basic `client.rs:47–51` (5). R—, L—. | CSV/JSONL opening `client.rs:444–499` (56); lazy writers `insert.rs:93,119`; Text `wire.rs:66,73`. Default JSON unused. | `client.rs:176–180` (5), no redirects. | F36–64 / U14,19 / M12 |
| Cloudflare Zones | Bearer `lib.rs:150–159` (10). `receive`, `policy.rs:98–136` (39); delay projection `ratelimit.rs:136–143` (8). L—. | JSON `lib.rs:70` (1); provider envelope remains. | `lib.rs:169–175` (7). | F64–84 / U56,63 / M23 |
| Datadog Events | A—; two-key construction/validation `credentials.rs:47–72` (26). R—, L—. | JSON `api.rs:34`; non-replayable JSON encoder `post.rs:120–126` (7). | `client.rs:87–92` (6). | F111–118 / U40 / M75 |
| DigitalOcean | Bearer `lib.rs:58–65` (8). R—, L—. | JSON `lib.rs:27` (1). | `lib.rs:97–106` (10). | F25–38 / U42 / M— |
| Discord REST | A—; Bot template `api.rs:46` (1). R—. L—; custom windows/global gates `limiter.rs:165–338` (174). | JSON `api.rs:47` (1). | README31 onward; caller disables redirects/retries. | F59–91 / U50 / M39 |
| Distribution Registry | Basic `auth.rs:158–162` (5); cache/challenge/acquisition `client.rs:147–278` (132). R—; one handwritten challenge replay. L—. | JSON/Binary/Encoded `api.rs:46,62,69,76,88–103`; ignored error media `error.rs:759`. | README54–57; caller client. | F118–119,184–198 / U93 / M23–24 |
| Dropbox | A—; Bearer template `wire.rs:30` (1). R—; numeric delay projection `error.rs:29–35` (7). L—. | JSON `wire.rs:31` (1). | README27–31; caller recipe. | F119–134 / U62 / M34 |
| Elasticsearch | Other auth scheme `model.rs:105–109` (5). R—, L—. | JSON/NDJSON `lib.rs:65,72` (2 declarations); bulk item semantics remain. | README23–27; caller recipe. | F13 / U14 / M41,66 |
| Europe PMC | A—. Retry parser `error.rs:18–30` (13), no engine. L—. | XML `xml.rs:1–183` (183); JSON `lib.rs:85,95,107,118`. | `lib.rs:132–140` (9). | F8,26–35 / U23 / M6 |
| Frankfurter | A—, R—, L—. | JSON `lib.rs:35` (1); exact-number model policy remains. | README24 onward; caller builder. | F47–52 / U45 / M— |
| FRED | A—; query key `lib.rs:78` (1). Retry parser inside `lib.rs:40–62` (23-line send policy), no engine. L—. | XML `codecs.rs:19–106` (88); mixed error document `108–144` (37); ZIP `146–221` (76). | `lib.rs:67–74` (8). | F9–16,55–67 / U9 / M34 |
| GitHub | A—; Bearer template `api.rs:61`, native header setup `context.rs:252–259` (8). R—; numeric projection `meta.rs:90,101`. L—. | JSON `api.rs:65`; download streaming remains SDK workflow. | Same-origin policy `context.rs:71–85` (15). | F103–120 / U∅ / M∅ |
| GitLab | A—; token validation `params.rs:229–238` (10). R—; numeric projection `meta.rs:81,99`. L—. | JSON `api.rs:53`; error-media override `send.rs:70`. | Same-origin policy `lib.rs:84–98` (15). | F87–94 / U43 / M19 |
| Google Drive | A—; Bearer template `api.rs:12` (1). R—; resumable probe/recovery is SDK state. L—. | Shared related multipart `upload.rs:78–103` (26); Encoded `api.rs:41`. | `client.rs:112–117` (6). | F18–24; F58 historical / U5 / M12,23 |
| HubSpot CRM | Cache/rotation `lib.rs:376–388,447–527` (94). R—; calls sent once. L—. | JSON/Form `lib.rs:26,54` (2). | README16–20; caller recipe. | F8–15,33–42 / U16–17 / M61–65 |
| Intercom | Bearer `lib.rs:106–113` (8). Retry parser `error.rs:122–131` (10), no engine. L—; observations only. | JSON `lib.rs:78` (1). | `lib.rs:118–124` (7). | F11–13,30–36 / U20 / M24–26,170 |
| Kraken Spot REST | A—; nonce/signing/credentials `auth.rs:1–105` (105). Retry parser `error.rs:105–114` (10), no engine. L—. | JSON/Form `lib.rs:35,51` (2). | `lib.rs:73–81` (9); TLS left explicit. | F59–91 / U69 / M57–58 |
| Kubernetes | A—; Bearer header `ids.rs:237–245` (9). R—, L—; watch continuation is separate. | JSON `api.rs:62`; consecutive JSON stream declarations `113,137,166,184,220,238`; reader alias `watch.rs:294`. | Caller client; no production builder. | F154–174 / U∅ / M∅ |
| Linear | A—; OAuth/header construction `lib.rs:255–268` (14). R—, L—. | GraphQL encoder starts `lib.rs:27`; JSON response `53`. | README14 onward; caller recipe. | F5–6 / U12 / M16 |
| Mailgun | Basic `lib.rs:125–126` (2). Retry parser `policy.rs:45–54` (10), no engine. L—. | Multipart adapter `message.rs:278–344` (67), including streamed branch `320–342` (23). | `lib.rs:127–137` (11). | F11–16,26–39 / U31 / M27–28 |
| Mastodon | Bearer `model.rs:116–120` (5). R—, L—. | JSON `api.rs:25`; SSE format/opening `streaming.rs:193–252` (60). | Caller policies documented `lib.rs:131` onward. | F74–84 / U26 / M3–4 |
| MediaWiki Action API | A—, R—, L—; public, single-attempt slice. | JSON `lib.rs:38` (1). | README27 onward; caller recipe. | F64–67 / U45 / M— |
| Microsoft Graph | A—; narrower token validation/header `client.rs:137–148` (12). R—, L—. | JSON `api.rs:10`; bounded error override `paging.rs:113`. | `client.rs:149–161` (13), HTTPS/loopback choices. | F35–40 / U81 / M58–60 |
| Nager Holidays | A—, R—, L—. | JSON `lib.rs:25` (1). | README25 onward; caller transport. | F16–17 / U6 / M— |
| NASA NeoWs | A—; query key `lib.rs:61–64` (4). R—, L—; quota projection `lib.rs:115–145` (31). | JSON `lib.rs:64` (1, overlapping preceding block). | README10 onward; caller recipe. | F— / U14,32 / M23–29 |
| Notion | A—; Bearer template `api.rs:18` (1). R—, L—. | JSON `api.rs:19` (1). | README39 onward; caller recipe. | F16 / U6 / M47 |
| ntfy | A—; token/header `model.rs:87–101` (15). R—; seconds-only delay accessor `error.rs:75–79`. L—. | SSE/NDJSON adapters `events.rs:63–112` (50); text encoder `model.rs:179` onward; JSON `lib.rs:130`. | README26 onward; caller transport. | F70–82 / U5,12 / M3,13 |
| Open Library | A—, R—, L—. | JSON `lib.rs:26` (1). | README27 onward; caller recipe. | F92 / U9 / M139 |
| Open-Meteo | A—, R—, L—. | Shared CSV plus two-table projection `csv.rs:1–207` (207); runtime JSON/CSV handling `lib.rs:96–126` (31). | Caller contract `README:17`. | F45–55 / U5 / M19–29 |
| OpenAI | A—; Bearer constructor `lib.rs:321–328` (8). R—, L—. | Manual batch JSONL `174–205` (32); shared SSE opening `341–360` (20); multipart `363–382` (20). | README13 onward; caller recipe. | F7–9 historical / U17 / M70–80 |
| PayPal | Basic `auth.rs:28–35` (8), acquisition `172–204` (33), cache access `policy.rs:110–124` (15). `receive_with`, `policy.rs:175–278` (104). L—. | JSON `lib.rs:68`; Form `auth.rs:77`. | `lib.rs:289–295` (7). | F27–43 / U20,33 / M50–52 |
| PokéAPI | A—, R—, L—. | JSON `lib.rs:82` (1). | README28 onward; caller recipe. | F3,20 / U21,25 / M— |
| Postmark | A—; vendor token `lib.rs:292` (1). R—; unsigned delay projection `272–274` (3). L—. | JSON `lib.rs:293` (1). | `lib.rs:15–19` (5). | F5–11 / U25,36 / M55–56 |
| QuickBooks Online | Cache/rotation state `auth.rs:519–583` (65), acquisition `662–746` (85). R—; manual one-401 replay `policy.rs:43–67` (25). L—. | Fault-aware JSON `format.rs:46–68` (23), ordinary JSON/Form `lib.rs:70–71,141–142`. | `lib.rs:168–174` (7). | F53–81 / U39 / M38 |
| REST Countries | Bearer helper `lib.rs:41–43` (3). R—, L—. | JSON `lib.rs:62` (1). | Caller contract README191–194. | F94–98 / U57 / M16 |
| Salesforce | A—; Bearer template `api.rs:42` (1). R—; job polling remains workflow. L—; quota observation only. | Runtime CSV helpers `csv.rs:183–237` (55); spool `247–341` (95); Binary upload `api.rs:153`. | Caller client; no production builder. | F50–108 / U∅ / M∅ |
| Sentry | Bearer `lib.rs:81–94` (14). `receive_with` policy `policy.rs:103–274` (172). Limiter setup `63–82` (20), observations `rate.rs:49–100` (52). | JSON `lib.rs:43` (1). | `lib.rs:99–106` (8). | F17–40 / U25–27 / M43–47 |
| Shopify | A—; token validation `scalars.rs:170–175` (6). R—, L—; GraphQL cost/throttle envelope retained. | GraphQL encoder `graphql.rs:62` onward; JSON `api.rs:34`; JSONL reader `bulk_export.rs:145–152` (8). | Caller recipe in `lib.rs:23` onward. | F75–79 / U∅ / M∅ |
| Slack | A—; Bearer template `wire.rs:38` (1). R—; manual delay parser `error.rs:37–55` (19). L—. | JSON `wire.rs:39` (1). | README18 onward; caller recipe. | F7–8,53–54 / U7 / M— |
| Snowflake | A—; Bearer header `client.rs:119–121` (3). R—; bounded polling begins `polling.rs:114`. L—. | Status-dependent JSON `api.rs:12–30` (19); partition/gzip `compression.rs:17–69` (53). | `client.rs:124–134` (11). | F78,112–148 / U43,48 / M70 |
| Spotify Web API | Basic/Bearer `auth.rs:81–90,381–400` (30); grant state `session.rs:118–194` (77). `run` policy `333–423` (91). L—. | Form/JSON `auth.rs:189–190`, JSON `lib.rs:83`. | README114–115; caller disables retries/redirects. | F50–104 / U13 / M50–53 |
| Square Payments | Bearer reexport `lib.rs:17` (1); static header `50`. R—, L—. | JSON `lib.rs:54` (1). | `lib.rs:40–46` (7). | F13,18–28 / U50 / M48 |
| Strava | Cache/rotation `auth.rs:182–222` (41); `run_tokio` policy `289–382` (94). Four-key rate observations `rate.rs:49–102` (54). | Form `lib.rs:128`; streamed multipart `upload.rs:85–101` (17). | `lib.rs:172–181` (10). | F9–16,21–25 / U19 / M222,232 |
| Stripe | A—; Bearer construction `context.rs:61–63` (3). R—; retry advice observation `send.rs:106–132` (27). L—. | Nested serializer `form.rs:184–728` (545); custom format `36–45` (10); JSON `lib.rs:182`. | Caller client; no production builder. | F129–154 / U∅ / M∅ |
| Twilio | Basic plus stricter checks `auth.rs:19–29` (11). R—, L—. | JSON/Form `lib.rs:30,35` (2). | README27 onward; caller recipe. | F101–110 / U55 / M— |
| USGS Earthquake Catalog | A—. Retry parser `error.rs:64–76` (13), no engine. L—. | CSV profile `csv.rs:42–51` (10), required-header wrapper `59–126` (68); JSON `lib.rs:34`. | README21 onward; caller recipe. | F12–17 / U42 / M112 |
| World Bank | A—, R—, L—. | JSON `lib.rs:188` (1). | README13; caller default builder. | F3 / U15,26 / M— |
| Xero Accounting | Basic/cache/acquisition `auth.rs:64–162` (99); durable journal `store.rs:1–343` (343). `run` policy `policy.rs:35–123` (89), including admission. | JSON `lib.rs:40`; Form `auth.rs:42`. | `lib.rs:90–96` (7). | F5–43 / U27–29 / M199–200 |
| Zendesk Support | Bearer `lib.rs:112–126` (15). `receive` read `policy.rs:72–96` (25), classifier `140–161` (22); writes bypass engine. L—. | Create-header JSON wrapper `codecs.rs:16–47` (32); JSON/text errors `58–95` (38); JSON `lib.rs:55`. | `lib.rs:136–142` (7). | F19–20,127–139 / U33 / M11 |

**What could shrink or disappear**

| Surface | Observed use | Assessment |
|---|---|---|
| `concord-extras` compatibility crate | **0 SDK dependencies and 0 Rust uses** found across `crates/` and `examples/`; only compatibility source and stale documentation names remain. `src/lib.rs:1–6`, manifest 29 lines. | Strongest removal candidate: deprecate/remove the compatibility facade in an appropriate breaking release. Removes **35 crate-file lines**, plus workspace/package references. Published external users remain unknown. |
| OpenAI `jsonl` feature | Enabled in `Cargo.toml`, but batch framing uses serde directly at `lib.rs:174–205`. | Remove the unused feature selection, or adopt the existing writer. No SDK Rust lines disappear merely by removing the flag. |
| ClickHouse default `json` | Enabled through Concord defaults; no `Json` codec use in production source. | Disable default features while retaining required transport features. No Rust deletion; dependency effect requires feature-graph measurement. |
| Mandatory `Encode::encode` refusal boilerplate | Mailgun `message.rs:281–285`: **5 lines**; Strava `upload.rs:87–89`: **3 lines**; shared multipart also refuses this path. | A default refusal would remove **8 SDK lines** without adding an item. It would also let an accidentally incomplete encoder compile and fail later. The saving does not clearly justify weakening that compile-time obligation. |
| Auth’s unconditional cache dependency | **13 of 20** auth adopters use headers without `TokenCache`; cache requires `futures-util` (`Cargo.toml:19`). | A cache feature could reduce package dependencies, but adds configuration and may save nothing after feature unification. Do not split into more crates without measured benefit. |
| `Limiter::available_leases` | **0 production SDK calls**, definition `lib.rs:244`. | Possible diagnostic API to make private in a breaking cleanup; no SDK-line saving. |
| `Limiter::remove` | **0 production SDK calls**, definition `lib.rs:282`. | Retain: bounded dynamic-key users need lifecycle removal; Discord migration would make that relevant. |
| `observe_global` | **1 SDK**, Strava `rate.rs:94,98`. | Retain: four-window 429 behavior is a concrete witness. |
| `authorization(scheme, token)` | **1 SDK**, Elasticsearch `model.rs:106`. | Retain: ApiKey is a distinct useful scheme; Discord Bot is another possible consumer. |
| `TokenCache::get` | **1 SDK**, registry `client.rs:156`. | Retain: cached challenge credentials avoid acquisition before the initial request. |
| `IgnoreContentType` | **10 SDKs** | Not dead surface. It expresses explicit error-document tolerance without weakening success decoding. Retain. |
| Retry entry points | All four have production adopters | Do not remove `run`, `run_tokio`, `receive`, or `receive_with` solely because each has few adopters. Their outcome and hook contracts differ. |

Several historical notes can also shrink: Drive’s manual multipart account; FRED’s private-media-helper complaint; async-hook requests now covered by `receive_with`; and retry README migration suggestions for numeric headers that now use `field::unsigned`. Keep historical evidence, but label its current status prominently.

**Crate-level disposition**

| Crate | Recommended direction |
|---|---|
| `concord-auth` | **Retain separately.** Improve acquisition/rotation documentation; investigate seed insertion only as a small, fallible operation. Keep persistence, signing and replay policy outside. |
| `concord-retry` | **Retain.** Clarify adapter selection and migrate matching SDKs before adding API. Keep seconds/date parsing available without transport/runtime dependencies. |
| `concord-ratelimit` | **Retain as optional.** Four distinct adopters justify it; 51 decliners do not justify installing guessed quotas. No merge into retry. |
| `concord-codecs` | **Retain feature separation.** Streamed multipart has the clearest repeated growth case. XML parsing and bounded upload collection are narrower candidates needing API tradeoff review. Leave provider completion, nested-form policy and archive membership outside core. |
| Core/macros | **Little growth justified by this theme.** Existing `SendWith`, prepared decomposition, `Decoder::execute`, `EncodedBody` and open codec traits preserve client → scope → endpoint. No auth/retry/rate DSL keywords are needed. |

**What this sweep could not decide**

- Net deletion counts without implementing migrations. Estimates above distinguish removable mechanics from gross provider-containing blocks.
- Whether published consumers prevent compatibility-crate or diagnostic-API removal.
- Actual dependency, binary-size or compilation savings from feature changes.
- Whether S3’s permissive media parameters, Salesforce’s CSV media parameter omission, or strict line-ending validation can change without compatibility impact.
- Whether an XML helper should publicly expose its DOM dependency.
- Runtime consequences of the two inferred cache ownership cycles.
- Current live-provider acceptance. Existing notes and test source were read as evidence; their recorded test results were not rerun.