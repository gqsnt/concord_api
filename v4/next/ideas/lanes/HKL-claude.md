# Idea lane H, K, L — provider claude

Baseline: `ba10189b` (worktree `work/wt/rv-p1`). Read: `CHARTER.md`, `IDEAS.md` (H, K, L),
`work/glue-dev/GLUE.draft.md`, `v4/next/inputs/CONSTRUCTION_PROBLEM.md`, `docs/CHEATSHEET.md`,
`docs/AUTHOR_GUIDE.md` (§1–4, 8–11), `crates/concord-macros/src/emit.rs` (doc generation
999–1642, 2372–2420; setter docs 170–215, 850–900), `syntax.rs` (path parsing), `model.rs`
(liveness); the sources, `FRICTION.md` and `MAINTENANCE.md` of mastodon, kubernetes,
mediawiki-action-api, fred, openai, world-bank, nager-holidays, pokeapi, frankfurter; and
distribution-registry, europe-pmc, datadog-events, adyen-checkout, notion, intercom where a design
needed a second witness. `work/inventory/FRICTION.md` did not exist, so the witnesses' own
`FRICTION.md`/`MAINTENANCE.md` were used.

**Observed** numbers come from scripts over the 55 `examples/*-sdk` crates at `ba10189b`
(`tools/glue/decl.py` parser, the glue classifier's per-line labels, `tools/glue/scoreboard.py`).
**Est.** marks an estimate. Nothing was built or run against a provider.

## Measured baseline for H, K, L (observed)

| Fact | Count |
|---|---|
| Endpoints / scopes / clients | 316 / 91 / 63 |
| Endpoint bodies holding only `path` | 153 |
| Endpoints with no `path` (they mean "the owner's URL") | 90 (50 with an empty body) |
| `path` equal to the endpoint's name | 46 endpoints; 26 scopes |
| Multi-line bodies holding only `path`/`request`/`response` | 85 (192 lines); 40 fit on one line in 100 columns (87 lines) |
| Endpoint `query { }` blocks of literal pairs only | 36 in 15 SDKs (`watch=1`, `format=json`, `file_type=json`, `list=search`, `uploadType=…`) |
| Query `append` / `override` / `remove` across owners | **0** in 55 SDKs (the 19 cross-owner operations are Stripe `stripe-account` **headers**) |
| Plain `query x` placements / keyed `query("k") x` | 263 / 130 (of 766 values) |
| Version literals in endpoint paths | 18 in 8 SDKs (Mastodon 7) |
| Declaration doc lines restating a route (`` `GET /…` ``) | 104 in 17 SDKs (Stripe 19, Kubernetes 15, GitHub 12) |
| Declaration doc lines explaining Concord mechanics (`decl_doc_mech`) | 81 |
| Doc lines outside the declaration restating a route (classifier `glue:docs`) | 120 in 30 SDKs |
| Value docs that begin by restating the value's wire key | 58 in 12 SDKs |
| Numeric range checks on inputs (`(1..=N).contains`) | ≈ 26 sites in ≈ 24 SDKs, mostly page sizes |
| Generated rustdoc words per endpoint | 543.8 → 888.7 during the loop (`v4/desirability/SUMMARY.md`) |
| DSL lines per endpoint (R4a) | 6.58 (2079 / 316) |

## Designs at a glance

| Id | Design | Fact | SDK lines removed (est.) | Also removes glue in | Verdict |
|---|---|---|---|---|---|
| H1 | The request line *(smaller than you think)* | what goes on the wire, rendered in 2–3 lines | ≈ 150–185 doc lines (81 mechanics + 104 route lines); generated words 889 → ≤ 150 | gives A, B, E, F one slot each instead of a paragraph | **recommend** |
| H2 | The declaration is the doc | the declaration excerpt itself | same as H1 | — | alternative to H1 |
| H3 | Routes as typed facts | method + route template, no values | ≈ 120 (`glue:docs`, Mastodon's table included) + PokeAPI 15 | **J, E, M, F** | **recommend** (with H1) |
| H4 | Value docs from the placement | where each value goes | 58 key prefixes, ≈ 10 lines | I | small, cheap |
| K1 | Path blocks `path "api/v1" { … }` *(smaller than you think)* | a shared prefix that is not a navigation level | Mastodon 7 → 2 version literals; Kubernetes 65 lines if it adopts | C, G | **recommend** |
| K2 | Query order is declaration order: keep the rule, delete the explanation *(smallest)* | wire order of query pairs | 0 SDK lines; ≈ 60 generated words per endpoint | H | **recommend** |
| K3 | Multi-segment path values `{each name}` | an identifier spanning several segments | registry ≈ 25–30 | C, G | narrow, cheap |
| K4 | Declared ranges `limit?: u8 in 1..=40` | a documented bound | ≈ 400–600 in ≈ 24 SDKs | **I, A, F, H** | **recommend, most glue** |
| K5 | Shared facts without navigation `with { … }` | a selector + format shared by siblings | declaration only (≈ 15 lines) | — | not recommended (order change) |
| L1 | One line per short endpoint: layout only *(smaller than you think)* | — | −87 DSL lines, 0 glue | — | **recommend** |
| L2 | Path in the method slot `GET("pods") pods(..)` | the path, where HTTP writes it | 0 glue; tokens, overlaps L1 | H (route docs redundant) | optional |
| L3 | Formats on the value they format `-> Vec<u8> as Binary` | the format of one body/output | 0 glue; 64 statements inline | — | optional |
| L4 | Conventions from names (path = name; unplaced = query) | — | — | — | **reject (data)** |

Mechanisms that remove glue in several families at once: **K4** (values, errors, paging, docs),
**H3** (docs, observability, rate limits, tests, URL following), **K1** (construction wrappers and
forwarders in Kubernetes). L removes no glue at all: its only measurable effect is declaration
lines, and most of that is layout (L1).

---

# Family H — Documentation: short, from the declaration

The problem is real and measured: generated docs grew 63 % (543.8 → 888.7 words per endpoint) to
satisfy model readers, and SDK authors still restate the route and Concord's mechanics by hand
(81 + 104 declaration doc lines, 120 SDK doc lines). Both have one cause: the generated page
explains *rules* instead of showing *the request*. When a reader can see the request, the rule
paragraphs and the author's restatements become redundant.

## H1 — The request line *(smaller than you think)*

**Fact it expresses:** what an endpoint puts on the wire (method, path template, query keys in wire
order, header names, which values are sensitive, body and output formats, the send result),
rendered from the resolved declaration in two or three lines.

**Declaration (author writes):** no syntax change. The declarations *shrink*, because the
mechanics notes authors wrote for readers become redundant. Nager
(`examples/nager-holidays-sdk/src/lib.rs:16-37`), before:

```rust
    /// Client for the unauthenticated Nager.Holidays Community v4 holiday endpoint.
    ///
    /// `NagerHolidays::new(http)` uses the hosted origin. For loopback tests or another
    /// deployment, `with_base_url(origin)` replaces the origin; the `/api/v4` prefix is
    /// declared on the operation. No auth or context configuration record is required.
    /// `send()` goes through the SDK's send policy [`KeepHead`].
    pub client NagerHolidays() {
        base "https://nagerholidays.com";
        send KeepHead;
        format Json;

        /// Retrieves one country/year array, in country-then-year path order.
        ///
        /// `country_code` is the provider's ISO 3166-1 alpha-2 spelling (for example `AT`).
        /// Values are sent as supplied, encoded as one path segment; this SDK does not
        /// claim to validate provider coverage or year availability. `send()` returns a
        /// [`HolidayResponse`] with the response metadata, or this SDK's [`Error`], which
        /// classifies HTTP 400 as an invalid request ([`KeepHead`]). `prepare()` builds the
        /// native request; `prepare()?.send()` returns Concord's own error instead.
        GET holidays(country_code: String, year: u16) -> Vec<Holiday> {
            path "api/v4/Holidays/{country_code}/{year}";
        }
    }
```

after (only provider facts remain):

```rust
    /// Client for the unauthenticated Nager.Holidays Community v4 holiday endpoint.
    pub client NagerHolidays() {
        base "https://nagerholidays.com";
        send KeepHead;
        format Json;

        /// Retrieves one country/year array, in country-then-year path order.
        ///
        /// `country_code` is the provider's ISO 3166-1 alpha-2 spelling (for example `AT`).
        /// This SDK does not validate provider coverage or year availability.
        GET holidays(country_code: String, year: u16) -> Vec<Holiday> {
            path "api/v4/Holidays/{country_code}/{year}";
        }
    }
```

Generated page of a request type. Before (Mastodon `ApiStatus`, est. ≈ 330 generated words in
12 paragraphs, assembled from the strings in `emit.rs:1215-1548`, not rendered): "Default request: `GET https://mastodon.invalid/api/v1/statuses/{id}`
(the declared base URL; the client's `with_base_url` replaces it). Construction or a send policy
may select or restrict the actual URL. / Request composition below describes `prepare()`, before
transport defaults or send-policy edits… / Each path hole's `Display` value is prepared as one
percent-encoded segment: every UTF-8 byte except RFC 3986 unreserved… / Headers, in chain order… /
No request body: no format-added `Content-Type`… / `Accept` fallback comes from the declared
response format… / Header notes describe `prepare()` before transport defaults… / Prepared
execution: … / `prepare()` bypasses the policy… / Operation: …". After (≈ 35 generated words):

```text
One status.                                            ← the author's first paragraph

GET {base}/api/v1/statuses/{id}
AUTHORIZATION: <sensitive> · accept: application/json (from Json)
send() → Result<Status, Error> via MastodonPolicy · body read ≤ 8 MiB · operation Api.status
How Concord builds, checks and sends requests: concord::api! § Requests
```

The whole rendering rule set:
- `{base}` is the client's base, printed once on the client page. A placeholder base (Mastodon's
  `https://mastodon.invalid`, `src/api.rs:23`) or a replaced base never appears as a URL; subdomain
  labels prefix it (`GET {region}.storage.{base}/…`).
- Query keys in wire order: a caller value as `key`, a literal as `key=value`, a sensitive value as
  `key=<sensitive>`, a `query(..)` value as `{name: Type}` (its type documents the keys).
- Header names as written (a literal, or the native constant's path such as `AUTHORIZATION`).
- Body: `body request: ResponseRequest as Json (application/json)`.
- Result: the declared `send` template if there is one, else today's policy projection (family B
  owns making it readable).
- The navigation method shows the author's first paragraph and the request line only.
- Everything else (escape set, `Accept`/`Content-Type` provenance, policy bypass, transport
  defaults) is written once on a shared page (the `concord::api!` rustdoc), linked from each line.

**Call site (caller writes):** unchanged.

**SDK code that disappears:** the doc lines become redundant and are deleted by their authors:
- 81 declaration doc lines that explain Concord mechanics in 40 SDKs (`decl_doc_mech`), e.g.
  `nager-holidays-sdk/src/lib.rs:18-21, 30-34`; `frankfurter-sdk/src/lib.rs:22-31`;
  `pokeapi-sdk/src/lib.rs:71-78`; `fred-sdk/src/lib.rs:79-82, 103, 118, 126, 139` ("The declared
  `Xml` format suggests `accept: text/xml`", four times); `openai-sdk/src/lib.rs:24-29, 39-40,
  46-47, 55-56`.
- 104 declaration doc lines restating the route in 17 SDKs, e.g. `kubernetes-sdk/src/api.rs:91-233`
  (15; "`listCoreV1Namespace`: `GET /api/v1/namespaces`" keeps the operationId and drops the
  route), `mastodon-sdk/src/api.rs:49, 70, 75, 81-82`.
- Est. 150–185 lines in total. Concord side: the per-endpoint prose of `emit.rs:999-1642` becomes a
  renderer of est. 150 lines; generated words per endpoint 889 → ≤ 150.

**Also removes glue in:** every later family. When A lands, the result line names the error
document; E adds `· idempotent`; F adds `· pages by starting_after`. Each family gets one slot on one
line instead of adding a paragraph (the H16s path).

**Hardest counterexample:** MediaWiki: four endpoints share one URL (no path) and differ by literal
pairs and a run-time-keyed `query(..) continuation` (`mediawiki-action-api-sdk/src/lib.rs:43-81`).
The line shows that without a rule:
`GET {base}?action=query&format=json&formatversion=2&srsearch&srlimit=10&{continuation: Continuation}&list=search&srprop=`,
including where the module selector `list=` sits. Kubernetes watch:
`…&allowWatchBookmarks=true&timeoutSeconds&watch=1`, the order `tests/declaration.rs:89-308` pins.
Mastodon: `{base}`, never the placeholder host its FRICTION.md item 1 complains about.

**Simple control (Nager):** declaration unchanged in shape; it may lose 9 doc lines (18–21,
30–34). Generated page: the author's doc plus three lines.

**Charter check:**
- C1: nothing is added to any declaration.
- C2: docs read the resolved facts; no behaviour.
- C3: the controls only shrink.
- C4: the URL and the query order are visible on every endpoint page; nobody needs the slot rule.
- C6: this is the point: ≤ 150 words per endpoint, the rules on one shared page.
- C7: `prepare()`, `into_parts()` and `send_raw()` are explained once on the shared page.
- C9: `tools/glue/rustdoc_metrics.py` (R4c) already measures words per endpoint; make it a gate at
  150. The existing composition oracle (`tests-gen/oracle_composition.py`) checks the rendered
  line against `prepare()` for generated declarations, so the line cannot lie.

**New concepts:** none for authors or callers; one shared rustdoc section.

**Breaking changes:** none in code. Concord's own doc-text tests in `emit.rs` (the
`request_summary_*`, `*_docs_*` tests, ≈ 3411–4183) are rewritten to the new text.

**Why it might be wrong:** H16s measured +25 pooled for model readers with the long prose; edge
answers (Accept provenance, `with_base_url` semantics) move one click away and readers may miss
them. The line is a small notation of its own (`{base}`, `<sensitive>`, `{name: Type}`).

## H2 — The declaration is the doc

**Fact it expresses:** the declaration itself. Each endpoint's rustdoc shows the resolved
declaration excerpt (every statement on the chain that reaches this endpoint, in source form)
instead of prose.

**Declaration (author writes):** no change. Generated page for Kubernetes `KubeCoreV1WatchPods`
(from `kubernetes-sdk/src/api.rs:55-62, 79-89, 94-114`), after:

```text
Declared as:
    client Kube(#[sensitive] header(AUTHORIZATION) authorization?: HeaderValue) {
        base "https://kubernetes.default.svc";
        format Json;
        scope core_v1 {
            path "api/v1";
            GET watch_pods(..WatchOptions) -> stream WatchEvent<Pod> {
                path "pods";
                query { "watch" = "1"; }
                headers { ACCEPT = "application/json"; }
                response JsonStream;
            }
        }
    }
    params WatchOptions(query("labelSelector") label_selector?: LabelSelector, …,
                        query("allowWatchBookmarks") allow_watch_bookmarks?: bool = true, …)
```

Siblings and statements that do not reach the endpoint are left out; doc comments are left out
(they are already on the page).

**Call site (caller writes):** unchanged.

**SDK code that disappears:** the same restatements as H1 (104 route lines, 81 mechanics lines):
the excerpt prints the path, the query, the headers and the formats verbatim. Concord side:
`emit.rs:999-1642` is replaced by a printer of the resolved model (the macro already prints types
and initializers with `type_text`/`expr_text`).

**Also removes glue in:** no extra family, but every future fact (A–F) is documented the day it is
added, because it is printed as written; no family needs its own doc generator.

**Hardest counterexample:** Stripe `headers { override "stripe-account" = each account_override else inherit; }`
(`stripe-sdk/src/lib.rs:209-476`, 19 times): the excerpt is exact, but the caller must know what
`each … else inherit` means, a DSL lesson for people who never write declarations. MediaWiki
`query(..) continuation?: Continuation`: the excerpt says where the pairs go, not which keys (the
type documents them, as today).

**Simple control (Nager):** unchanged declaration; the page shows the 5-line excerpt (base, send,
format, endpoint).

**Charter check:**
- C1: nothing added.
- C4: the source of truth is on the page.
- C6: an excerpt that needs a paragraph exposes a declaration that is wrong, instead of hiding it
  under generated prose.
- C9: docs cannot drift from the declaration; test that the excerpt re-parses to the same model.

**New concepts:** none.

**Breaking changes:** none.

**Why it might be wrong:** callers are not authors: `..Group`, `else inherit` and `response F` are
Concord vocabulary, and "what URL?" must be evaluated in the reader's head (C4 holds for authors,
not necessarily for callers). Deep chains (Kubernetes namespaced + group) make the excerpt longer
than the answer.

## H3 — Routes as typed facts

**Fact it expresses:** each endpoint's method and route template (`GET api/v1/statuses/{id}`, with
literal query selectors: `GET api/v1/pods?watch=1`), a static value that contains no request value.

**Declaration (author writes):** no change. Generated (new):

```rust
// per endpoint, on concord::GeneratedEndpoint
const ROUTE: concord::Route;                 // "GET", "api/v1/statuses/{id}", selectors []
// per client: one row per endpoint, declaration order, with the navigation path
pub const ROUTES: &'static [concord::Route];
// in core, from the static EndpointMeta every Operation already points to
impl Operation { pub fn route(&self) -> Option<&'static Route> }
```

The client's rustdoc gets one generated table (navigation, route, output) built from `ROUTES`;
H1's request line is its per-endpoint view.

**Call site (caller writes):**

```rust
// before: only the declared name exists
tracing::warn!(operation = err.operation().map(|o| o.name()), "request failed");
// after: a low-cardinality, secret-free route (OpenTelemetry's client attribute `url.template`)
tracing::warn!(url.template = err.operation().and_then(|o| o.route()).map(Route::template), "request failed");
```

**SDK code that disappears:**
- Hand-written operation tables: `mastodon-sdk/src/lib.rs:40-50` (11 lines, classifier `glue:docs`
  44–50) and its per-method route notes `lib.rs:157, 162, 168, 173, 192, 203, 219`; the other
  `glue:docs` lines (120 in 30 SDKs, e.g. `datadog-events-sdk/src/client.rs:19, 98, 126, 134`,
  `distribution-registry-sdk/src/blob.rs:26, 39, 87, 97`) link the client's table instead.
- README operation tables (outside `src`): Mastodon's `MAINTENANCE.md` lists "operation tables"
  among the places each new endpoint had to be added.
- With an optional `Route::matches(&Url)`: `pokeapi-sdk/src/lib.rs:247-261` (the path check of a
  server-returned URL, 15 lines).

**Also removes glue in:** **J** (span names and metrics without secrets: a template holds hole
names, never values, so the canary can test it); **E** (Discord keys its limiter on
`RouteKey { name, method }`, `discord-rest-sdk/src/limiter.rs:78-84, 153`, which is Discord's own
"route" notion); **M** (a snapshot test of `ROUTES` catches an accidental route change);
**F** (a `next` URL can be checked against the route it must match, as PokeAPI does by hand).

**Hardest counterexample:** MediaWiki: every endpoint has the same path (none). The literal query
pairs (`list=search`, `prop=revisions`) are part of the route as selectors, so `search` and
`category_members` differ. Mastodon keeps its generated client private (family C), so its table is
invisible to callers until C makes the generated client public: H3 helps Mastodon only after C.

**Simple control (Nager):** `NagerHolidays::ROUTES` has one row; declaration unchanged.

**Charter check:**
- C2: "typed access to the facts" in the charter's own words; tracing and limiting stay in Rust.
- C3: no change.
- C4: the route is the declared path.
- C7: callers can build their own requests against a declared route.
- C8: constants.
- C9: test that `ROUTES` equals `prepare()`'s path with the holes substituted (oracle), and canary
  that no route string contains a planted secret.

**New concepts:** `concord::Route`, `Operation::route()`, `ROUTE`, `ROUTES`.

**Breaking changes:** none (`EndpointMeta` is `__private`).

**Why it might be wrong:** where a send policy rewrites the URL (Mastodon's streaming host,
Registry's `Location` follow) the route names the declared request, not the URL actually used. A
string index invites `if route == "…"` dispatch, which `operations Name;` already does better with an
exhaustive `match`.

## H4 — Value docs from the placement

**Fact it expresses:** where each value goes on the wire (query key, header name, path hole,
template), stated by the generated setter and config-field docs.

**Declaration (author writes):** before (`mastodon-sdk/src/api.rs:32-47`):

```rust
        params TimelineOptions(
            /// `local`: only statuses of this instance.
            query local?: bool,
            /// `remote`: only statuses of other instances.
            query remote?: bool,
            // …
            /// `limit`: at most this many statuses (1 to 40; the server's default is 20).
            query limit?: TimelineLimit,
        );
```

after:

```rust
        params TimelineOptions(
            /// Only statuses of this instance.
            query local?: bool,
            /// Only statuses of other instances.
            query remote?: bool,
            // …
            /// At most this many statuses (1 to 40; the server's default is 20).
            query limit?: TimelineLimit,
        );
```

Generated setter doc before (`emit.rs:866-871`): "`local`: only statuses of this instance. /
Sets `local` (present). / Initially absent." After: "Only statuses of this instance. / Sends
`local=<value>` in the query. Initially absent." Header: "Sends header `AUTHORIZATION`
(sensitive)." Template: "Used in header `authorization` as `Bearer {token}`." `query(..)`: "Writes
its own query pairs (see `Continuation`)."

**Call site (caller writes):** unchanged.

**SDK code that disappears:** 58 value docs in 12 SDKs lose their key prefix
(`mastodon-sdk/src/api.rs:33-46, 59-63`, `kubernetes-sdk/src/api.rs:83-85`,
`zendesk-support-sdk/src/lib.rs:69-75`, `quickbooks-online-sdk/src/lib.rs:111`); est. 10 lines
deleted outright where the key was the whole doc.

**Also removes glue in:** I (the setter of a value says what it sends, so wrapper setters that
exist only to document a key lose their reason).

**Hardest counterexample:** Kubernetes `query("labelSelector") label_selector?`: name and key
differ, and the setter page is the only place a caller sees `labelSelector` next to
`set_label_selector`; today the author writes it by hand. Group values: the trait method and every
including endpoint get the same line.

**Simple control (Nager):** unchanged declaration; its path values get "Path segment
`{country_code}`."

**Charter check:** C4 (value → wire visible at the setter); C6 (one line per setter); C1/C2
untouched.

**New concepts:** none.

**Breaking changes:** none.

**Why it might be wrong:** a small gain (prefixes, not lines). The key in the author's doc also
documents the provider's spelling for readers of the declaration itself, who do not see setter
pages.

**Seeds, H:** seed 1 is H1; seed 2 is H2; seed 3 (remove the SDK-local notes) is a consequence of
H1 or H2 (81 lines now, not 76); seed 4 ("ask whether the rule should change") is answered in K2
(query order: no) and K3 (encoding: keep one value, one segment; add a visible exception).

---

# Family K — Domain facts and request composition

## K1 — Path blocks: `path "api/v1" { … }` *(smaller than you think)*

**Fact it expresses:** a path prefix several endpoints share without being a level the caller
navigates: an API version, a Kubernetes group/version, a collection.

**Declaration (author writes):** before (`mastodon-sdk/src/api.rs:49-101`, docs trimmed):

```rust
        GET public_timeline(..TimelineOptions) -> Vec<Status> {
            path "api/v1/timelines/public";
        }
        GET hashtag_timeline(hashtag: Hashtag, /* any[], all[], none[] */ ..TimelineOptions) -> Vec<Status> {
            path "api/v1/timelines/tag/{hashtag}";
        }
        GET status(id: StatusId) -> Status {
            path "api/v1/statuses/{id}";
        }
        GET instance() -> InstanceInfo {
            path "api/v2/instance";
        }
        GET public_stream(query only_media?: bool) -> stream StreamEvent {
            path "api/v1/streaming/public";
            response PublicStreamFormat;
        }
        GET hashtag_stream(query tag: Hashtag) -> stream StreamEvent {
            path "api/v1/streaming/hashtag";
            response PublicStreamFormat;
        }
        GET local_hashtag_stream(query tag: Hashtag) -> stream StreamEvent {
            path "api/v1/streaming/hashtag/local";
            response PublicStreamFormat;
        }
```

after:

```rust
        path "api/v1" {
            GET public_timeline(..TimelineOptions) -> Vec<Status> { path "timelines/public"; }
            GET hashtag_timeline(hashtag: Hashtag, /* … */ ..TimelineOptions) -> Vec<Status> {
                path "timelines/tag/{hashtag}";
            }
            GET status(id: StatusId) -> Status { path "statuses/{id}"; }
            GET public_stream(query only_media?: bool) -> stream StreamEvent {
                path "streaming/public";
                response PublicStreamFormat;
            }
            GET hashtag_stream(query tag: Hashtag) -> stream StreamEvent {
                path "streaming/hashtag";
                response PublicStreamFormat;
            }
            GET local_hashtag_stream(query tag: Hashtag) -> stream StreamEvent {
                path "streaming/hashtag/local";
                response PublicStreamFormat;
            }
        }
        GET instance() -> InstanceInfo { path "api/v2/instance"; }
```

The rule, in one sentence: *`path "p" { … }` prefixes `p` to the endpoints and scopes inside it; it
is not a level the caller navigates and has no values, type or setters.* Holes in `p` read the
enclosing owner's values.

Kubernetes may use it for one namespace handle across groups (`kubernetes-sdk/src/api.rs:94-241`):

```rust
        path "api/v1" { GET pods(..ListOptions) -> PodList { path "pods"; } /* … */ }
        path "apis/apps/v1" { GET deployments(..ListOptions) -> DeploymentList { path "deployments"; } /* … */ }

        /// One namespace, in every group.
        scope namespaced(namespace: NamespaceName) {
            path "api/v1/namespaces/{namespace}" {
                GET pods(..ListOptions) -> PodList { path "pods"; }
                GET pod(name: ObjectName) -> Pod { path "pods/{name}"; }
                /* services, service, config_maps, config_map, watch_pods */
            }
            path "apis/apps/v1/namespaces/{namespace}" {
                GET deployments(..ListOptions) -> DeploymentList { path "deployments"; }
                GET deployment(name: ObjectName) -> Deployment { path "deployments/{name}"; }
                /* watch_deployments */
            }
            path "apis/batch/v1/namespaces/{namespace}" { /* jobs, job */ }
            path "apis/networking.k8s.io/v1/namespaces/{namespace}" { /* ingresses, ingress, watch_ingresses */ }
        }
```

**Call site (caller writes):**
- Mastodon: unchanged (`mastodon.status(id)`).
- Kubernetes: `team_a.apps().deployments()` (through `NamespaceContext`) or
  `kube.client().apps_v1().namespaced(ns).deployments()` → `kube.namespaced(ns).deployments()`;
  cluster-wide `kube.core().pods()` → `kube.pods()`.
- Adyen (`adyen-checkout-sdk/src/lib.rs:40-41`, a version written as a navigation step):
  `checkout.v71().payments(key, request)` → `checkout.payments(key, request)`.

**SDK code that disappears:**
- Version literals in endpoint paths: 18 → est. 12 (Mastodon 7 → 2; Datadog's `v2/events` and
  `v2/events/search` inside `scope events`, `datadog-events-sdk/src/api.rs:42-60`, share one
  `path "v2/events" { … }`).
- Kubernetes, if it adopts the one-namespace layout: `kubernetes-sdk/src/context.rs:68-98` (27 lines:
  `core`/`apps`/`batch`/`networking`/`namespace` forwarders) and `context.rs:109-152` (38 lines:
  `NamespaceContext` and its `Debug`): 65 lines, plus `lib.rs:12-14`.

**Also removes glue in:** C and G (Kubernetes's context struct is a wrapper the scoreboard counts in
D1b, held only to rebind one value across four scopes); L (shorter path strings).

**Hardest counterexample:** Kubernetes. Its URL nests the namespace *inside* the group prefix
(`apis/apps/v1/namespaces/{ns}/deployments`) while callers navigate namespace first, so any tree
repeats one fact. Today the namespace scope is declared four times (`api.rs:123, 170, 197, 224`)
plus a 65-line context; with path blocks each group prefix is written twice (cluster-wide and
namespaced: 8 literals instead of 4). K1 makes the choice possible, not free, and its own FRICTION.md
item 3 calls today's layout "mostly intentional". Plurals that exist in two groups (core `events`,
`events.k8s.io` `events`) would need distinct names in one namespace scope. Mastodon's
api/v1 + api/v2 is the clean case: moving an endpoint between versions is moving one line. The
query order is untouched (blocks carry only a path), so Kubernetes `watch=1` keeps its place.

**Simple control (Nager):** unchanged: one endpoint, its `api/v4/…` path stays where it is.

**Charter check:**
- C1: a provider fact (a prefix).
- C3: none.
- C4: the URL is the concatenation in nesting order, as for scopes; "a scope keeps meaning
  navigation" (IDEAS K) holds, because a block is not a scope.
- C5: an empty block, or a hole naming a value its enclosing owner does not have, is an error at
  that token.
- C6: one sentence.
- C8: static.

**New concepts:** the block form of `path`.

**Breaking changes:** none (additive). SDKs that adopt it change their navigation (Kubernetes,
Adyen).

**Why it might be wrong:** two ways to share a prefix (scope `path` with navigation, block without),
and authors may hide a level callers need (a Kubernetes group). For Kubernetes the trade is honest
but not a win on version literals.

## K2 — Query order is declaration order: keep the rule, delete the explanation *(smallest)*

**Fact it expresses:** the order of query pairs on the wire (it matters for signatures, caches and
exact-string tests; servers rarely care).

**Observation.** Today's rule ("each key keeps the position of its first appearance on the chain;
occurrences in contribution order", `docs/AUTHOR_GUIDE.md` §4.7, and the per-endpoint paragraph at
`emit.rs:1211`) differs from plain reading order only when an owner `append`s to, `override`s or
`remove`s a key an outer owner set. **No query key in the 55 SDKs does that** (0 uses; the 19
cross-owner operations are Stripe `stripe-account` headers, whose order between names is
unspecified anyway). So for every SDK request, wire order is already source order, root first:
client values and block, each scope's values and block, the endpoint's parameters (groups and
`query(..)` where written), then the endpoint's block.

**Declaration (author writes):** unchanged. The rule is restated everywhere as one sentence: *"Query
pairs are sent in the order they are declared, from the client down to the endpoint; a key changed
lower down (`append`, `override`) keeps the place where it was first declared."* H1 shows the result
on each endpoint page. World Bank (`world-bank-sdk/src/lib.rs:192-196`):

```rust
        GET indicator(country: String, indicator: String, query date: String,
                      query page: u32 = 1, query per_page: u32 = 50) -> WireResponse {
            path "country/{country}/indicator/{indicator}";
            query { "format" = "json"; }
        }
```

sends `?date=…&page=1&per_page=50&format=json`, the order it is written in. Its author expected
`format=json` first and corrected the test (`world-bank-sdk/FRICTION.md` item 1,
`tests/protocol.rs:47-51`); with H1 the page shows `?date&page=1&per_page=50&format=json` before any
test is written.

**Call site (caller writes):** unchanged.

**SDK code that disappears:** none (no glue depends on the rule). Docs: the paragraph at
`emit.rs:1210-1211` (≈ 60 words on every endpoint with a query) goes; AUTHOR_GUIDE §4.7 shrinks to
the sentence and its example; §11.1's "the query order changes" stays and is predicted by the
sentence.

**Also removes glue in:** H (answers H seed 4: the rule is already declaration order, only its
description was hard).

**Hardest counterexample:** Kubernetes `…&allowWatchBookmarks=true&timeoutSeconds=300&watch=1`
(`kubernetes-sdk/tests/declaration.rs:301`, `tests/streaming_lists.rs:163`) and MediaWiki
`…&srsearch=…&srlimit=10&list=search&srprop=` (`mediawiki-action-api-sdk/tests/protocol.rs:76-84`)
keep exactly their bytes. Rejected alternative, "literal pairs first" (World Bank's intuition, and
the order MediaWiki's own examples use): it reorders up to 36 endpoints in 15 SDKs and the pinned
Kubernetes strings (`tests/declaration.rs:89-301`, `tests/streaming_lists.rs:66-173`), a wire
change for taste.

**Simple control (Nager):** unchanged.

**Charter check:** C4 (predictable from reading order); C6 (one sentence instead of a 70-word
paragraph); C8; C9 (the §4.7 doctest and `tests-gen/oracle_composition.py` keep pinning it).

**New concepts:** none.

**Breaking changes:** none.

**Why it might be wrong:** the second clause still describes a slot, and the one place it bites (an
endpoint changing a client key, as the demo `examples/sdk/src/lib.rs:194` does with
`override "tag" = each tags else inherit`) may still surprise the first SDK that uses it.

## K3 — Values that are several path segments: `{each name}`

**Fact it expresses:** a provider identifier that spans several path segments (a Docker repository
name `acme/tools/app`), each segment still escaped on its own.

**Declaration (author writes):** before (`distribution-registry-sdk/src/api.rs:32-106`; three
clients because "a Concord path value is always exactly one segment", `api.rs:12-16`):

```rust
concord::api! {
    pub(crate) client Registry(#[sensitive] header(AUTHORIZATION) authorization?: HeaderValue) {
        base "https://registry.invalid";
        GET ping() { path "v2/"; }
        GET catalog(query n?: PageSize, query last?: String) -> CatalogBody {
            path "v2/_catalog";
            response Json;
        }
    }
}

concord::api! {
    /// One repository, at the base URL `<registry>/v2/<name>`.
    pub(crate) client Repository(#[sensitive] header(AUTHORIZATION) authorization?: HeaderValue) {
        base "https://registry.invalid/v2/repository";
        GET tags(query n?: PageSize, query last?: String) -> TagsBody {
            path "tags/list";
            response Json;
        }
        /* manifest, put_manifest, blob_head, blob, start_upload, upload_chunk, upload_status,
           complete_upload */
    }
}
```

after:

```rust
concord::api! {
    pub(crate) client Registry(#[sensitive] header(AUTHORIZATION) authorization?: HeaderValue) {
        base "https://registry.invalid";
        GET ping() { path "v2/"; }
        GET catalog(query n?: PageSize, query last?: String) -> CatalogBody {
            path "v2/_catalog";
            response Json;
        }

        /// One repository: one path segment per component of its name.
        scope repository(name: RepositoryName) {
            path "v2/{each name}";
            GET tags(query n?: PageSize, query last?: String) -> TagsBody {
                path "tags/list";
                response Json;
            }
            /* the same eight endpoints, unchanged */
        }
    }
}
```

The rule: *`{each x}` in a path sends one segment per item of `x`, each escaped as any path value;
an empty, `.` or `..` item, or no item at all, is a `Build` error.* `x` iterates by reference
(`IntoIterator` of `Display` items, as `each` already requires for query and headers);
`RepositoryName` adds `impl<'a> IntoIterator for &'a RepositoryName` over the components its
constructor already validates (`reference.rs:24`).

**Call site (caller writes):** `let repo = registry.repository(&name)?;` (fallible: it builds a base
URL, `client.rs:116-124`) → `let repo = registry.repository(&name);`.

**SDK code that disappears:** `distribution-registry-sdk/src/client.rs:110-124` (`repository()`,
15 lines → ≈ 5: no base string, no `with_base_url`, no `# Errors`); `api.rs:51-58` (the second
client's header, 7 lines); the "three clients" explanation `api.rs:7-17` (≈ 8 lines). Est. 25–30
lines. Not addressed: `client.rs:390-404` `verbatim_last_segment` (the digest's `:` that Concord
escapes). Sending the RFC 3986 `pchar` set raw would fix it but also sends `+` raw, which some
servers read as a space (a known S3 pitfall; inferred, not tested here), so it is not proposed as a
default.

**Also removes glue in:** C (one less place where an SDK rebuilds a base URL by hand to work around
the path model, the same pattern as Mastodon's instance base); G (the `Repository` wrapper keeps
only its token-scope role).

**Hardest counterexample:** S3 object keys and GitLab project paths contain `/` but must stay one
segment (`%2F`): `each` is opt-in per hole, so `{key}` keeps today's behaviour. The H16 surprise
("`a/b` becomes `a%2Fb`") stays the default, now with a visible exception spelled at the hole.

**Simple control (Nager):** unchanged.

**Charter check:**
- C1: a fact about the identifier's shape.
- C4: visible at the hole (`{each name}` reads as "several segments").
- C5: the build error names the item's position, not the value if sensitive.
- C6: one sentence, reusing `each` ("one occurrence per item" for query and headers today).
- C9: wire test with `acme/tools/app`; tests that `..` and empty components are refused before I/O.

**New concepts:** `each` inside path templates.

**Breaking changes:** none.

**Why it might be wrong:** one witness today. Splitting is delegated to the SDK type, so a type that
yields `"a/b"` as one item still gets `%2F` (safe, but surprising).

## K4 — Declared ranges: `query limit?: u8 in 1..=40`

**Fact it expresses:** a documented bound on an input (page sizes: Mastodon 1..=40, FRED 1..=1000
and 1..=100000, Notion 1..=100, Intercom 1..=150, GitHub 1..=100, …).

**Declaration (author writes):** before (`mastodon-sdk/src/api.rs:45-46` and the type in
`model.rs:265-308`, 37 non-blank lines):

```rust
            /// `limit`: at most this many statuses (1 to 40; the server's default is 20).
            query limit?: TimelineLimit,
```

```rust
pub struct TimelineLimit(u8);
impl TimelineLimit {
    pub const MAX: u8 = 40;
    pub fn new(limit: u8) -> Result<Self, InvalidLimit> { /* 0 or > MAX → InvalidLimit */ }
    pub fn get(self) -> u8 { self.0 }
}
impl fmt::Display for TimelineLimit { /* … */ }
pub struct InvalidLimit;
impl fmt::Display for InvalidLimit { /* "a timeline limit is 1 to 40 statuses" */ }
impl std::error::Error for InvalidLimit {}
```

after:

```rust
            /// At most this many statuses (the server's default is 20).
            query limit?: u8 in 1..=40,
```

FRED (`fred-sdk/src/lib.rs:104, 127` and the `limit!` macro `lib.rs:149-184`):

```rust
        GET search(query search_text: String, query limit: SearchLimit = SearchLimit::default(), ..OffsetOptions) -> SearchPage
        // + macro_rules! limit { … }  limit!(SearchLimit, 1000, …);  limit!(ObservationLimit, 100000, …);
```

→

```rust
        GET search(query search_text: String, query limit: u32 in 1..=1000 = 1000, ..OffsetOptions) -> SearchPage
```

The rule: *`in a..=b` bounds a value; outside it, `prepare()` is a `Build` error naming the value and
the bound, and nothing is sent.* Constant bounds, any `PartialOrd` value type; on an `each` value
the bound applies to each item.

**Call site (caller writes):** `.set_limit(TimelineLimit::new(40)?)` (`mastodon-sdk/src/lib.rs:30`)
→ `.set_limit(40)`. The failure moves from `InvalidLimit` at construction to a `concord::Error` of
kind `Build` at `prepare()`/`send()` (the SDK's `From<concord::Error>` already carries it).

**SDK code that disappears:** `mastodon-sdk/src/model.rs:265-308` (37) and two names in the
re-exports `lib.rs:104-108`; `fred-sdk/src/lib.rs:149-184` (36) and `error.rs:47-48, 71` (3);
`notion-sdk/src/params.rs:5-40` (31); `intercom-sdk/src/models.rs:7-30` (21). Across the ≈ 24
SDKs with a page-size check (`discord-rest-sdk/src/model.rs:300`,
`digitalocean-sdk/src/lib.rs:209`, `github-sdk/src/params.rs:190`,
`europe-pmc-sdk/src/input.rs:124`, `dropbox-sdk/src/models.rs:15`, `gitlab-sdk/src/params.rs:348`,
`stripe-sdk/src/types.rs:124`, `slack-sdk/src/lib.rs:49`, `zendesk-support-sdk/src/models.rs:405`, …)
est. 400–600 lines. The classifier labels these model/behavior, not glue; by C2 they restate a
provider fact.

**Also removes glue in:** **I** (a bare `u8` accepts literals, `Default::default()` and `.parse()?`,
which the newtypes block; IDEAS I's own complaint); **A** (error enums lose their range variants:
`fred-sdk/src/error.rs:47`, Intercom `Error::PageSize`, Notion `PageSizeError`); **F**
(`concord-pagination` can read the declared maximum page size instead of SDK constants); **H** (the
request line and the setter show `limit (1..=40)`).

**Hardest counterexample:** MediaWiki `srlimit`, whose maximum depends on the caller's rights (500,
or 5000 for bots): a conditional bound is not a fact the declaration can state, so it stays
undeclared, like Kubernetes `limit` (no documented maximum). Mastodon's 40 is the documented
default of an instance setting; an instance that raises it would see valid requests refused.

**Simple control (Nager):** unchanged (`year: u16` needs no bound).

**Charter check:**
- C1: a documented fact.
- C2: the same kind of check Concord already makes before sending (empty or `..` segments, header
  names, `limit` on bodies); no policy, no retry.
- C4: visible on the value.
- C5: the error names the value, the bound and the endpoint, never a generated type.
- C8: deterministic.
- C9: compile-fail test for a non-constant bound; wire test that 41 is refused before I/O; canary
  that a `#[sensitive]` value's bound error does not print the value.

**New concepts:** `in <range>` after a value's type.

**Breaking changes:** none in Concord. SDKs that adopt it change setter argument types (callers drop
`TimelineLimit::new(..)?`) and lose typed error variants.

**Why it might be wrong:** it moves an SDK's choice of strictness (check early, or let the server
answer) into the declaration and makes every client pre-reject. The newtype failed at construction
with a value callers could store and pass around; the range fails only at `prepare()`.

## K5 — Shared request facts without navigation: `with { … }`

**Fact it expresses:** request facts several siblings share: the representation selector (FRED
`file_type=json` with `Json`, Europe PMC `format=json`, USGS `format=geojson`), Mastodon's stream
format, Kubernetes's watch selector. K1 is its special case `with { path "p"; … }`.

**Declaration (author writes):** before (`fred-sdk/src/lib.rs:114-135`):

```rust
        scope series(query series_id: String) {
            path "series";
            GET metadata() -> Metadata;
            GET metadata_json() -> Metadata {
                query { "file_type" = "json"; }
                response concord::Json;
            }
            GET observations(/* … */) -> Observations {
                path "observations";
            }
            GET observations_json(/* … */) -> Observations {
                path "observations";
                query { "file_type" = "json"; }
                response concord::Json;
            }
            /* export_csv */
        }
```

after:

```rust
        scope series(query series_id: String) {
            path "series";
            GET metadata() -> Metadata;
            GET observations(/* … */) -> Observations { path "observations"; }
            with {
                query { "file_type" = "json"; }
                response concord::Json;
                GET metadata_json() -> Metadata;
                GET observations_json(/* … */) -> Observations { path "observations"; }
            }
            /* export_csv */
        }
```

The rule: *`with { … }` holds statements (`path`, `query`, `headers`, formats) shared by the
endpoints inside it; it is not navigated and has no values.*

**Call site (caller writes):** unchanged.

**SDK code that disappears:** declaration lines only, ≈ 15 across 4 SDKs: FRED −4;
`europe-pmc-sdk/src/lib.rs:82-96, 116-124` (four `format=json` + `response Json` pairs → one);
USGS; Mastodon's `response PublicStreamFormat;` ×3 (`api.rs:88, 94, 100`) → 1. No glue outside the
declaration.

**Also removes glue in:** none.

**Hardest counterexample:** Kubernetes `watch=1` and FRED `file_type`. A key declared in a `with`
block is declared before the endpoint's parameters, so by K2's rule it is sent first: FRED's
`…&limit=2&offset=0&file_type=json` (`fred-sdk/tests/first_use.rs:289-304`) becomes
`file_type=json&limit=2&offset=0`; Kubernetes, if it grouped its watches, would send
`watch=1&allowWatchBookmarks=true…` and change its pinned strings
(`tests/declaration.rs:89-301`, `tests/streaming_lists.rs:66-173`). Kubernetes cannot group them
anyway: its six watches live under six different owners. The change is predictable (C4) but real,
which is why K1 (path only, no order effect) is the recommended form.

**Simple control (Nager):** unchanged.

**Charter check:** C1 (facts); C4 (declaration order still decides); C6 (a third grouping construct
next to scopes and parameter groups); C3 none.

**New concepts:** the `with` block.

**Breaking changes:** none; an adopting SDK changes its wire order.

**Why it might be wrong:** scope, parameter group and `with` are three kinds of grouping, one too
many for C6; the gain is ≈ 15 declaration lines.

**Seeds, K:** seed 1 is K1 (spelled as a block form of `path`, not a new keyword, and limited to the
path so the query order cannot move); seed 2 is K2, which finds the rule already is declaration
order for every SDK and changes only its wording. Rejected: "literal pairs first" (K2), changing the
default escape set (K3).

---

# Family L — Declaration syntax and conventions

L removes no glue: glue lives outside the declaration. Its measurable effect is declaration lines
(R4a) and how quickly a reader finds the URL. The data says most of the line gain is layout.

## L1 — One line per short endpoint: layout, no language change *(smaller than you think)*

**Fact it expresses:** none new.

**Observation:** 85 endpoint bodies holding only `path`, `request` or `response` span several lines
(192 lines). Within the repository's `max_width = 100` (`rustfmt.toml`), 40 of them fit on one line,
which saves 87 lines; 91 path-only bodies are already one line. rustfmt does not format `api!`
bodies, so this is the author's choice, not the language's.

**Declaration (author writes):** before (`mastodon-sdk/src/api.rs:51-53, 71-73, 77-79`):

```rust
        GET public_timeline(..TimelineOptions) -> Vec<Status> {
            path "api/v1/timelines/public";
        }
        GET status(id: StatusId) -> Status {
            path "api/v1/statuses/{id}";
        }
        GET instance() -> InstanceInfo {
            path "api/v2/instance";
        }
```

after:

```rust
        GET public_timeline(..TimelineOptions) -> Vec<Status> { path "api/v1/timelines/public"; }
        GET status(id: StatusId) -> Status { path "api/v1/statuses/{id}"; }
        GET instance() -> InstanceInfo { path "api/v2/instance"; }
```

(OpenAI already writes `GET content(id: String) -> Vec<u8> { path "files/{id}/content"; response Binary; }`,
`openai-sdk/src/lib.rs:57`.) Mechanism: the AUTHOR_GUIDE and CHEATSHEET show the one-line form
first; a check built on `tools/glue/decl.py` (which already parses bodies) reports a short body
spread over lines when it would fit in 100 columns. No macro change.

**Call site (caller writes):** unchanged.

**SDK code that disappears:** −87 declaration lines in est. 15 SDKs (R4a 6.58 → 6.30); 0 glue.

**Also removes glue in:** none.

**Hardest counterexample:** long headers: FRED's `search`/`search_json` lines are 129 and 134
columns already (`fred-sdk/src/lib.rs:104, 108`), the registry's do not fit either
(`distribution-registry-sdk/src/api.rs:44-104`); they stay as they are. C4 is unaffected.

**Simple control (Nager):** unchanged (its one-line form would be 121 columns).

**Charter check:** C3 (controls unchanged); C1, C2, C4, C6 untouched.

**New concepts:** none.

**Breaking changes:** none.

**Why it might be wrong:** style drifts unless checked; 4 % fewer lines does not change what a
reader must understand, so the R4a metric overstates its value.

## L2 — The path in the method slot: `GET("statuses/{id}") status(..)`

**Fact it expresses:** the endpoint's path, written where HTTP writes it (next to the method), in the
`keyword("literal") name` shape Concord already uses for `query("per-page") per_page` and
`header(AUTHORIZATION) token`.

**Declaration (author writes):** before (`kubernetes-sdk/src/api.rs:126-138`):

```rust
                /// `listCoreV1NamespacedPod`.
                GET pods(..ListOptions) -> PodList { path "pods"; }

                /// `readCoreV1NamespacedPod`.
                GET pod(name: ObjectName) -> Pod { path "pods/{name}"; }

                /// `listCoreV1NamespacedPod` with `watch=1`: `GET .../pods?watch=1`.
                GET watch_pods(..WatchOptions) -> stream WatchEvent<Pod> {
                    path "pods";
                    query { "watch" = "1"; }
                    headers { ACCEPT = "application/json"; }
                    response JsonStream;
                }
```

after:

```rust
                /// `listCoreV1NamespacedPod`.
                GET("pods") pods(..ListOptions) -> PodList;

                /// `readCoreV1NamespacedPod`.
                GET("pods/{name}") pod(name: ObjectName) -> Pod;

                /// `listCoreV1NamespacedPod` with `watch=1`.
                GET("pods") watch_pods(..WatchOptions) -> stream WatchEvent<Pod> {
                    query { "watch" = "1"; }
                    headers { ACCEPT = "application/json"; }
                    response JsonStream;
                }
```

Mastodon: `GET("api/v1/statuses/{id}") status(id: StatusId) -> Status;` reads exactly like its doc
comment "`GET /api/v1/statuses/{id}`" (`api.rs:70`), which can go. Without a path,
`GET metadata() -> Metadata;` (`fred-sdk/src/lib.rs:119`) and MediaWiki's four endpoints are
unchanged: no parentheses after the method still means "the owner's URL". Scopes keep their `path`
statement.

**Call site (caller writes):** unchanged.

**SDK code that disappears:** 0 glue. Declaration: the 153 path-only bodies lose `{ path …; }`
(≈ 8 characters each); multi-line ones become one line where they fit (overlaps L1); the 104 route
restatements in declaration docs become plainly redundant, since the route is now the second token
of the line.

**Also removes glue in:** H (route restatements in docs).

**Hardest counterexample:** MediaWiki: pathless endpoints must stay unambiguous: `GET search(` and
`GET("…") search(` differ visibly. Trailing slashes keep their rule (endpoint's own path only):
`GET("v2/") ping();` (registry), `GET("pokemon/{id}/") pokemon(id: String) -> Pokemon;` (PokeAPI).

**Simple control (Nager):** `GET("api/v4/Holidays/{country_code}/{year}") holidays(country_code: String, year: u16)`
`-> Vec<Holiday>;` goes from 3 lines to 2 and loses `{ path …; }`: it does not grow, but its form
changes if the statement form is retired.

**Charter check:**
- C3: no growth. If the endpoint `path` statement is retired, the controls change form (fewer
  tokens): the one C3-sensitive point. The additive variant leaves them untouched but gives
  endpoints two spellings (C6).
- C4: the path is the second token of the endpoint, as in the provider's own docs.
- C5: an endpoint with both forms is an error at the second, naming the first.

**New concepts:** `METHOD("path")`.

**Breaking changes:** if the endpoint `path` statement is retired, 226 endpoints migrate
mechanically (a compile error that prints the rewritten line); additive variant: none.

**Why it might be wrong:** two spellings of one fact (scopes use the statement, endpoints the slot);
long paths push the endpoint name to the right, where readers scan for it.

## L3 — Formats written on the value they format: `-> Vec<u8> as Binary`

**Fact it expresses:** the wire format of one body or one output, written next to that type instead
of in a separate statement. Defaults on clients and scopes (`format Json;`) stay statements.

**Declaration (author writes):** before (`openai-sdk/src/lib.rs:48-57`):

```rust
        POST upload(body form: MultipartBody) -> FileObject {
            path "files";
            request Multipart;
        }
        GET content(id: String) -> Vec<u8> { path "files/{id}/content"; response Binary; }
```

after (shown with L2):

```rust
        POST("files") upload(body form: MultipartBody as Multipart) -> FileObject;
        GET("files/{id}/content") content(id: String) -> Vec<u8> as Binary;
```

Kubernetes: `GET("pods") watch_pods(..WatchOptions) -> stream WatchEvent<Pod> as JsonStream { … }`;
Mastodon: `-> stream StreamEvent as PublicStreamFormat` (`api.rs:86-88, 92-94, 98-100`); a media
type: `body change: Rename as Json "application/merge-patch+json"` (today's
`request Json "application/merge-patch+json";`).

**Call site (caller writes):** unchanged.

**SDK code that disappears:** 0 glue. Declaration: the 64 endpoint-level `request`/`response`
statements become two words on the line they qualify; 31 bodies holding only a path and a format
become one line (72 lines if all fit; est. 30 at 100 columns, overlapping L1).

**Also removes glue in:** none.

**Hardest counterexample:** an untyped `-> stream` (`openai-sdk/src/lib.rs:41-44`) has no format to
name. A body whose media type is chosen at run time (`request Multipart;`) keeps its meaning.

**Simple control (Nager):** unchanged (its format comes from `format Json;` on the client).

**Charter check:** C4 (the format sits next to the type it decodes: the CHEATSHEET's "the format,
not the Rust type, decides the wire" becomes visible on one line); C6 (one sentence); C3 none.

**New concepts:** `as F` on bodies and outputs.

**Breaking changes:** if endpoint `request`/`response` statements are retired, 64 statements
migrate; additive: none.

**Why it might be wrong:** `as` reads like a Rust cast; two places for formats (statement defaults,
`as` on one value) is one more rule; `as F "media/type"` crowds a long body type.

## L4 — Conventions derived from names (path = name; unplaced value = query): evaluated, rejected

**Fact it expresses:** claimed: "the path is the endpoint's name" (seed L2); also tested: "a value
neither placed nor referenced is a query parameter named like the value".

**Declaration (author writes):** if adopted, before (`kubernetes-sdk/src/api.rs:99`)
`GET namespaces(..ListOptions) -> NamespaceList { path "namespaces"; }` → after
`GET namespaces(..ListOptions) -> NamespaceList;`, and `GET search(query srsearch: String, …)` →
`GET search(srsearch: String, …)`.

**Call site (caller writes):** unchanged.

**SDK code that disappears:** none. path = name applies to 46 of 316 endpoints (15 %) and saves
≈ 10 characters each, no lines (they are one-liners already). Unplaced = query would remove 263
`query` words.

**Also removes glue in:** none.

**Hardest counterexample:** 90 endpoints (28 %) have no path and mean "the owner's URL": MediaWiki
4/4 (`lib.rs:47-81`), FRED `metadata`/`metadata_json` (`lib.rs:119-124`), Stripe ×10, Shopify ×11
(GraphQL, one URL), Salesforce ×9, QuickBooks ×4. A default path silently moves all 90
(`GET search()` would call `api.php/search`), or every one of them needs a new spelling such as
`path "";`. Unplaced = query removes the "value is never used" error (`model.rs:619-635`), the check
that catches a hole forgotten in a template today: `GET status(id: StatusId) { path "statuses"; }`
would silently send `?id=…`.

**Simple control (Nager):** unchanged (its path is not its name).

**Charter check:** path = name fails C4 (a reader cannot tell whether a pathless endpoint means its
owner's URL or its name) and costs a migration to save characters; unplaced = query fails C5 (a
forgotten path hole no longer fails where it was written).

**New concepts:** none (rejected).

**Breaking changes:** path = name changes the URL of 90 endpoints.

**Why it might be wrong (the rejection):** a provider that names every route after its operation
(a JSON-RPC-like API) would gain more than these 55 SDKs show; the evidence here is only the corpus
we have.

**Seeds, L:** seed 1 (inline path) is L2, but L1 shows layout gets most of its lines without a
language change; seed 2 (path defaults to the name) is rejected with data in L4; seed 3 (one-line
forms) is L1 + L2 + L3. The `send policy -> Result<_, Error>;` line the IDEAS problem statement
cites is one line per client and a real fact (it is what makes the generated `send()` signature
readable); nothing to remove.

---

## Recommended combination and what to measure next

- **Do first (no syntax, no breakage):** H1 (request line + shared page + 150-word gate), K2 (one
  sentence for query order), L1 (layout). Together they cut generated words per endpoint from 889
  to ≤ 150, make 185 SDK doc lines redundant, and remove 87 DSL lines.
- **Then the facts that remove glue across families:** K4 (ranges: est. 400–600 lines in ≈ 24 SDKs,
  also I, A, F, H) and H3 (routes: also J, E, M, F).
- **Small, additive structure:** K1 (path blocks; Mastodon's api/v1 + api/v2, Datadog's v1 + v2,
  optionally Kubernetes's namespace). K3 if a second multi-segment provider appears.
- **Not recommended:** K5 (moves keys on the wire, a third grouping construct), L4 (fails C4/C5).
  L2/L3 are optional syntax: they buy readability, not lines.
- **To verify before deciding (not done here):** render H1 on three SDKs and rerun the H16 reader
  tasks against the 150-word pages (the risk named in H1); run `tools/glue/rustdoc_metrics.py`
  before and after; prototype K4 on Mastodon and FRED and count the lines with the classifier.
