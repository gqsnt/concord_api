# H12 design survey — one-place versioning / shared prefix (base `f921b002`)

Read-only survey by a Codex lane ({'model': 'gpt-6.1-sol', 'effort': 'xhigh', 'sandbox': 'read-only'} 2026-11-09T01:06:04Z 2026-11-09T01:14:57Z) for the coordinator; not a review or a decision. Its final message, verbatim:

**H12 is weak as a case for new DSL syntax.** There are production duplication opportunities, but client `path` already provides a shared prefix without an extra navigation call. Initialized values cover repeated version segments across differently shaped paths. The strongest comparator is therefore current Concord plus consistent SDK authoring guidance.

Surveyed `f921b002`; nothing edited or compiled. “Observed” below means source or existing test assertions. Proposed compiler/API effects are expectations.

**Observed mechanism and inventory.** Paths compose base → client → scopes → endpoint. The model collects those segments; the emitter generates their construction; core appends them to the base path (`crates/concord-macros/src/model.rs:1388`, `emit.rs:2938`, `crates/concord-core/src/__private.rs:568`). An existing test asserts the complete composition (`crates/concord/tests/composition.rs:45`).

The author guide already recommends putting a version prefix in client `path` when it must survive `with_base_url` (`docs/AUTHOR_GUIDE.md:334`). Constants can initialize declared values; descendants reference those values using `{client.version}` or `{scope.version}` (`docs/AUTHOR_GUIDE.md:148`, `:535`). Each hole represents one encoded segment, so an ordinary value containing `api/v1` cannot substitute two path segments (`v4/SEMANTICS.md:132`; existing encoding assertions: `crates/concord/tests/composition.rs:57`).

Inventory notation:

- SDK names mean `examples/<name>-sdk/`; citations within each row are relative to that directory, normally under `src/`.
- **C/S/E/B/H/Q** = client path / scope path / endpoint path / base path / header / query.
- Counts are occurrences of the listed version or API-prefix fact in production declarations, excluding comments, tests and docs. Each occurs once unless marked `×n`. Additional handwritten sites are identified separately.
- Repeated resource suffixes such as `issues/{id}` belong to H06’s scope-flattening question; they are not counted as API-version duplication here.

| SDK | Current version/prefix expression and count; evidence |
|---|---|
| adyen-checkout | **S** `v71`; scope itself is named `v71` (`lib.rs:42`). |
| airtable | **E** `v0/{base}/{table}` (`lib.rs:215`). |
| amazon-s3 | No versioned path prefix; ListObjectsV2 selects **Q** `list-type=2` (`api.rs:37`, `:74`). SigV4 is separate. |
| anthropic-messages | **C** `v1`; **H** initialized from `API_VERSION = "2026-10-21"` (`lib.rs:60`, `:81`, `:84`). |
| arxiv | **B** `/api` (`lib.rs:30`). |
| backblaze-b2-native | **C** `b2api/v4` (`wire.rs:22`). Upload requests subsequently follow the issued URL, preserving its path/query (`wire.rs:59`). |
| clickhouse | No versioned/shared API path prefix (`wire.rs:36`). |
| cloudflare-zones | **C** `client/v4` (`lib.rs:69`). |
| datadog-events | **C** `api`; **E** `v2/events` family **×2**, legacy `v1/events` once (`api.rs:36`, `:59`, `:63`, `:69`). |
| digitalocean | **C** `v2` (`lib.rs:28`). |
| discord-rest | **B** `/api/v10` (`api.rs:45`). |
| distribution-registry | **E** `v2` **×2**, plus repository placeholder **B** `/v2/repository` (`api.rs:41`, `:45`, `:57`). Handwritten repository-base construction adds another site (`client.rs:113`); collection checks add two (`pages.rs:128`, `:169`). |
| dropbox | **B** `/2` (`wire.rs:29`). |
| elasticsearch | No versioned/shared API prefix; resource paths only (`lib.rs:63`, `:77`). |
| europe-pmc | **B** `/europepmc/webservices/rest` (`lib.rs:74`). |
| frankfurter | **E** `v2/rate` **×2** (`lib.rs:43`, `:51`). `API_VERSION = "2.1.1"` describes OpenAPI `info.version`, a different fact (`lib.rs:13`). |
| fred | **C** `fred`; no numeric route version (`lib.rs:77`, `:80`). |
| github | **H** from `ApiVersion::pinned()`; literal authority `PINNED_TEXT = "2026-03-10"` (`api.rs:55`, `:63`; `params.rs:330`). |
| gitlab | **B** `/api/v4`; public `GITLAB_COM` repeats the full base literal, giving two source sites (`api.rs:35`, `:51`). |
| google-drive | **S** `drive/v3` **×3**, including `upload/drive/v3` (`api.rs:16`, `:28`, `:44`). Upload-path validator repeats it once (`upload.rs:183`). |
| hubspot-crm | **S** `crm/v3/objects/contacts`; separate OAuth **S** `oauth/2026-09` (`lib.rs:29`, `:50`). |
| intercom | **H** literal `intercom-version = "2.17"` (`lib.rs:77`). |
| kraken-spot-rest | **C** `0` (`lib.rs:34`). Signing reads the prepared request’s path rather than repeating the prefix (`policy.rs:78`). |
| kubernetes | Four **S** prefixes: `api/v1`, `apis/apps/v1`, `apis/batch/v1`, `apis/networking.k8s.io/v1` (`api.rs:96`, `:156`, `:191`, `:210`). Four `v1` tokens, representing independently versioned groups. Server release `v1.37.2` is separate (`lib.rs:76`). |
| linear | **C** `graphql`; no route version (`lib.rs:51`). |
| mailgun | **C** `v3` (`lib.rs:66`). |
| mastodon | **E** `api/v1` **×6**, `api/v2` once; common `api` head **×7** (`api.rs:60`, `:75`, `:80`, `:86`, `:95`, `:101`, `:107`). |
| mediawiki-action-api | **B** `/w/api.php`; response-shape **Q** `formatversion=2` on the query scope (`lib.rs:37`, `:44`). |
| microsoft-graph | **B** `/v1.0`; public constructor repeats the full versioned base and overrides the generated base (`api.rs:9`; `client.rs:88`, `:162`). Loopback validation repeats `/v1.0` (`client.rs:113`). |
| nager-holidays | **E** `api/v4`; public `API_VERSION = "v4"` is separate from that route literal: two source sites (`lib.rs:13`, `:36`). |
| nasa-neows | **B** `/neo/rest/v1` (`lib.rs:62`). |
| notion | **B** `/v1`; independent **H** version initialized from `NOTION_VERSION = "2026-03-11"` (`api.rs:15`, `:17`; `lib.rs:25`). |
| ntfy | No route version; `DOCUMENTED_VERSION = "v2.37.2"` identifies the documentation release (`lib.rs:98`, `:108`, `:113`). |
| open-library | No versioned/shared API prefix; resource paths only (`lib.rs:25`, `:39`, `:49`). |
| open-meteo | **E** `v1/forecast` (`lib.rs:36`). |
| openai | **C** `v1` (`lib.rs:28`). |
| paypal | Main client **S** `v2` **×4**: two checkout-order paths and two payment paths (`lib.rs:77`, `:97`, `:138`, `:168`). Separate auth client **E** `v1/oauth2/token` (`auth.rs:76`). |
| pokeapi | **B** `/api/v2` (`lib.rs:80`). |
| postmark | No versioned/shared API prefix; resource paths only (`lib.rs:291`, `:300`). |
| quickbooks-online | **C** `v3/company/{connection}`; **Q** `minorversion=75`; separate token-client **C** `oauth2/v1/tokens` (`lib.rs:71`, `:72`, `:143`). |
| rest-countries | **C** `countries/v5` (`lib.rs:61`). |
| salesforce | **C** `services/data/v{version}`; version is a required client value (`api.rs:42`, `:44`). Typed `PINNED` authority lives in `ids.rs:64`; link validation derives its segment from context (`context.rs:335`). |
| sentry | **C** `api/0` (`lib.rs:42`). |
| shopify | **S** `admin/api/{client.version}/graphql.json`; client default uses typed `ApiVersion::PINNED` (`api.rs:30`, `:45`; `scalars.rs:29`, `:35`). |
| slack | **B** `/api` (`wire.rs:37`). |
| snowflake | **C** `api/v2/statements` (`api.rs:39`). |
| spotify-web-api | Main-client **C** `v1`; auth client separately uses `api/token` (`lib.rs:82`; `auth.rs:188`). |
| square-payments | **C** `v2`; header date literal and public `API_VERSION` repeat `"2026-09-16"`: two sites for the header-version fact (`lib.rs:30`, `:52`, `:53`). |
| strava | Main-client **C** `api/v3`; separate OAuth-client scope `oauth` (`lib.rs:77`, `:129`). |
| stripe | **B** `/v1`, repeated in handwritten `DECLARED_BASE`; independent **H** default derives from `PINNED_API_VERSION` (`lib.rs:175`, `:180`; `context.rs:41`; `types.rs:11`). |
| twilio | **C** `2010-04-01/Accounts/{account_sid}` (`lib.rs:28`). |
| usgs-earthquake-catalog | **C** `fdsnws/event/1` (`lib.rs:34`). |
| world-bank | **B** `/v2` (`lib.rs:186`). |
| xero-accounting | **C** `api.xro/2.0` (`lib.rs:39`). |
| zendesk-support | **C** `api/v2` (`lib.rs:54`). |

These are independently counted source sites, not a reconstruction of baseline D2a’s `18 / 5 / 5` diagnostic (`v4/desirability/BASELINE.md:37`).

**Observed propagation boundaries.** A client/scope path edit feeds generated descendants through composition. Handwritten URL construction, validators, exposed constants, tests and authored documentation have independent dependencies.

Concrete repetitions relevant to the baseline’s six flagged SDKs are:

| SDK | Current repeated test/document fact |
|---|---|
| github | Header date in `tests/support/mod.rs:106`, `tests/download.rs:193`, `tests/operations.rs:252`. |
| kraken | Paths/signature input in `tests/protocol.rs:59`, `:78`, `:137`; production signer derives the path (`src/policy.rs:78`). |
| kubernetes | Full paths throughout `tests/declaration.rs:63`; provider/path inventory in `PROVIDER.md:34`. |
| nager | Constant pin asserted independently at `tests/protocol.rs:23`, route repeated at `:47`; documented route at `README.md:5`. |
| notion | Literal header date in `tests/first_use.rs:51`, while `tests/protocol.rs:92` already derives it from `NOTION_VERSION`; README repeats the date (`README.md:4`). |
| sentry | Route literals in `tests/protocol.rs:42`; maintenance documentation repeats the prefix (`MAINTENANCE.md:30`). |

These are current source observations, not an identification of protected task edits. Historical provenance/schema-version records also repeat versions; a current-version change does not automatically make those records stale.

**Production deficit: limited maintenance exposure is present; an unmet DSL capability is not established.**

- **Observed:** Frankfurter has two independent production route literals for the same pair-rate family. Drive has three versioned scope paths plus a matching upload validator.
- **Expected consequence:** changing only one Frankfurter route, or only some Drive routes, would leave production requests on mixed versions. Updating Drive’s declarations without its validator would leave upload-session acceptance inconsistent.
- **Limit:** no current wrong request path was reproduced here. Existing mechanisms can centralize these facts: Frankfurter can move `v2/rate` into client `path`; Drive can initialize one version value used by all three templates and share its constant with the validator.

The baseline needs two separate readings. Its missed-propagation flags are explicitly supporting evidence about tests/docs, not missed production paths (`BASELINE.md:128`). Separately, it records Kubernetes e1’s behavioural oracle failure—only 1/4 assertions passed because a moved group reached some paths (`:123`). That is recorded evidence of an incomplete edit, but it does not establish why existing group-level paths were insufficient. The task details were not inspected.

**Options and expected costs/breakages.**

| Option | Benefit and concept cost | Expected breakages / migration |
|---|---|---|
| **(a) Keep current + guidance** | Use client/existing-scope `path`; initialize repeated version segments from SDK constants; derive routine test expectations from those constants. No new DSL concept. | Guidance alone: none. Same-wire SDK refactoring should preserve callers and wire assertions. Retain one independent provider-pin check; deriving every oracle from the implementation would hide an incorrect shared constant. |
| **(b) Declare inherited version/prefix at client or scope** | As worded, already available through `path`. A dedicated `version` keyword needs an additional benefit to justify placement/default/exception rules. | An additive, opt-in statement should preserve existing method names, types and operation identity. Moving existing endpoints into a newly introduced scope is a different change and is expected to break navigation. |
| **(c) Static prefix aliases, or an SDK-local dependency check** | Macro-owned literal aliases could share several segments without runtime values/setters. A check could enforce declared fact-to-endpoint coverage without new request syntax. Costs: alias/reference rules, exception handling, or a maintained fact manifest. | Opt-in use should preserve existing APIs and wire paths. Arbitrary Rust prefix expressions would make macro-time path validation harder; that tradeoff needs a separate card. A mandatory global-version rule would break legitimate mixed-version designs. |

Useful current-API comparisons:

- **Frankfurter:** two route-prefix literals → one client prefix, with no added arguments/navigation.
- **Drive:** three version literals → one initialized value; the handwritten upload validator must share its authority.
- **Datadog:** centralize the read-version fact while retaining legacy posting v1.
- **Mastodon:** centralize common `api` and/or v1 separately; preserve the v2 instance endpoint.
- **PayPal:** a main-client `v2` prefix can cover all four current scopes while the separate auth client retains v1. Equal version numbers alone do not prove checkout and payment upgrades must always move together.

For an **actual version change**, the known repeated test/doc surfaces of those witnesses are below. Paths again belong to the named SDK; tests are under `tests/`.

| SDK | Test files containing current route facts | Authored documents containing those facts |
|---|---|---|
| frankfurter | `first_use.rs:38`, `protocol.rs:39`, `workflows.rs:22` | `README.md:16`, `MAINTENANCE.md:13`, `FIRST_USE.md:23`; schema provenance is a separate fact. |
| google-drive | `canary.rs:93`, `first_use.rs:97`, `maintenance.rs:20`, `protocol.rs:47`, `support/mod.rs:55`, `workflows.rs:27` | `FIRST_USE.md:6`, `README.md:52` |
| datadog-events | `canary.rs:112`, `first_use.rs:134`, `post.rs:60`, `post_consumer.rs:12`, `protocol.rs:97`, `workflows.rs:102` | `FIRST_USE.md:16`, `README.md:25`, `PROVENANCE.md:26` |
| distribution-registry | `canary.rs:37`, `first_use.rs:85`, `protocol.rs:49`, `support/mod.rs:308`, `workflows.rs:99` | `FIRST_USE.md:15`, `README.md:18`, `FRICTION.md:56`, `PROVENANCE.md:28` |
| mastodon | `first_use.rs:87`, `hashtag_use.rs:19`, `hashtags.rs:102`, `protocol.rs:75`, `workflows.rs:27` | `FIRST_USE.md:37`, `README.md:6`, `MAINTENANCE.md:42`, `FRICTION.md:46`, `PROVENANCE.md:15` |
| paypal | `authorizations.rs:18`, `canary.rs:134`, `first_use.rs:63`, `protocol.rs:55`, `support/mod.rs:31` | `FIRST_USE.md:20`, `README.md:7`, `PROVENANCE.md:16` |

A same-wire refactor should leave these wire expectations intact. Historical documents require classification before updating.

Two migration costs deserve explicit kill-condition treatment:

- **Removing Adyen’s version scope:** expect `.v71()` and `CheckoutV71*` references to break in `tests/{canary,capture,first_use,protocol,workflows}.rs`; public-shape references occur in `README.md:14`, `:67`, `FIRST_USE.md:11`, `MAINTENANCE.md:149`, `FRICTION.md:41`. This is scope/API migration cost, already within H06’s subject (`v4/desirability/reviews/h06-design-survey.md:138`).
- **Moving prefixes from base into client path:** this changes what `with_base_url` replaces. Fixtures currently supplying the prefix would need adjustment, including Discord `tests/support/mod.rs:75`, Dropbox `:26`, GitLab `:46`, Graph `:27`, Stripe `tests/wire.rs:24` and `tests/canary.rs:137`. Such relocation should not be bundled into a supposedly transparent refactor.

Mastodon’s v1/v2 mixture must remain expressible. Kubernetes must retain independent core/apps/batch/networking prefixes, namespaced and cluster-wide operations, and `/version/`; one global `v1` authority would conflate distinct contracts (`kubernetes-sdk/src/api.rs:92`, `:96`, `:156`, `:191`, `:210`). Its resource `apiVersion` constants are another surface, separate from request routing (`src/model/resources.rs:20`, `:329`).

**Automatic measurement.** A raw “one literal remains” check cannot decide H12. It can miss an endpoint disconnected from the authority, count overwritten placeholders as authorities, or incorrectly combine Kubernetes groups.

A stronger static-first card could measure:

1. **Authoritative semantic sites:** independently name each provider fact and its affected endpoint set; count definitions and verify every intended route depends on the definition.
2. **Propagation coverage:** compare composed paths before/after one fact change, checking affected endpoints and unchanged exceptions. Include handwritten builders/validators explicitly; the macro cannot infer their relationship.
3. **Detection stage:** seeded partial changes should be caught by the declared static check or independent path assertions. Record gaps rather than assuming compilation catches them.
4. **Scope/API cost:** compare generated navigation, type names, operation identities, constructor arguments and controls. Proposed compilation outcomes remain expectations until tested.
5. **Docs/tests:** report duplicated current-contract expectations separately from historical provenance. Fewer textual occurrences alone is not success.

Use **Frankfurter and Drive** as unlike witnesses: flat repeated routes versus several route shapes plus a validator. Use **Mastodon as the contrary case**, with Kubernetes as an additional independent-group guard. Use **Nager and PokeAPI as simple controls**; Frankfurter must also gain no required config or navigation ceremony. Kraken is a useful positive control for derived handwritten behaviour: signing already consumes the prepared path.

A dependency-and-coverage check could decide the narrow edit-site outcome if its endpoint/fact mapping is independent. I4 e1 remains an agent editing instrument; it is useful only for residual questions about editors finding and completing those dependencies. Current source evidence supports trying the existing-API comparator first and **KEEP_CURRENT** unless a candidate demonstrates a benefit that it cannot provide.