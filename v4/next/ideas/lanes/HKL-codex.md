# H, K, L — codex idea lane

Repository evidence: ba10189b, read-only. Three alternative designs per family; these are proposals, not implemented or tested changes. All source paths and line ranges below are relative to /home/canarit/projects/Concord_Next/work/wt/rv-p1. “Before” excerpts reproduce source declarations, sometimes omitting surrounding, unchanged owners and author comments. “After” is proposed syntax unless explicitly described as already supported.

The charter, IDEAS.md, construction problem, glue inventory, language guides, witness declarations, relevant macro emission code, and SDK-local friction/maintenance evidence informed these proposals. The combined inventory/FRICTION.md does not exist. Historical friction reports describe older trees; current source wins. No other idea lane or design output was consulted.

The inventory measures 120 nonblank SDK documentation-glue lines, versus 3,182 declaration lines. Among these witnesses it labels Mastodon 16 documentation lines and Kubernetes 5, but FRED, OpenAI, MediaWiki and World Bank zero. Thus removing a repeated declaration or a misleading paragraph is useful without being a measured runtime-glue reduction. Estimates below keep those categories separate and are not additive across alternative designs.

Two baseline details determine the designs:

- Kubernetes actually prepares allowWatchBookmarks=true&watch=1, pinned at examples/kubernetes-sdk/tests/declaration.rs:87-90, 104-107, 137-139, 152-154, 177-179 and 194-196. A visually attractive watch-first order would change bytes.
- MediaWiki's protocol comparison sorts pairs at examples/mediawiki-action-api-sdk/tests/protocol.rs:29-37. It establishes multiplicity and values, not raw list= order. Its present composition is inherited action/format/formatversion, endpoint inputs, continuation pairs, then list/srprop. Preserve that order and add independent raw-target assertions.

The three controls stay exactly as written: examples/nager-holidays-sdk/src/lib.rs:15-39, examples/pokeapi-sdk/src/lib.rs:70-88, and examples/frankfurter-sdk/src/lib.rs:19-54. No proposal requires their migration, extra declarations, or new caller concepts. In particular, missing path continues to mean “append nothing,” as FRED metadata requires.

# Family H — documentation

## H1. Delete the repeated manual, keep a compact contract — smaller than you think

Fact it expresses: Each operation has a method, declared URL, input shape, output format and send contract, already present in the declaration.

Mechanism: Change only documentation emission. Keep the author's purpose once, a method/URL line, the signature's required inputs, a compact optional-input/default list, formats/limits and the send result. Put encoding, inheritance, transport defaults and policy-bypass mechanics on one shared, versioned Concord documentation page. Link to it with one short sentence. No documentation configuration enters api!.

Set a CI budget of at most 150 generated words over an endpoint's navigation entry, request-type summary and send documentation together, including tables/code text. Do not duplicate author prose on all three surfaces. Track authored prose separately; never silently truncate provider caveats to meet a generator budget. Do not describe policy execution as necessarily one HTTP attempt: only the prepared/default path makes that guarantee.

Declaration (author writes), OpenAI, examples/openai-sdk/src/lib.rs:38-44:

~~~rust
// Before
/// Open a response event stream: the unread body, for [`EventStream`].
/// Sends `content-type: application/json` from `Json` and the authored
/// `accept: text/event-stream`; this untyped stream adds no format suggestion.
POST response_stream(body request: ResponseRequest) -> stream {
    path "responses";
    headers { "accept" = "text/event-stream"; }
}

// After: the wire declaration is identical.
/// Open a response event stream: the unread body, for [`EventStream`].
POST response_stream(body request: ResponseRequest) -> stream {
    path "responses";
    headers { "accept" = "text/event-stream"; }
}
~~~

Generated entry: “Open a response event stream. POST https://api.openai.com/v1/responses. Body: ResponseRequest via Json. Accept: text/event-stream. send: Result<Response<concord::Incoming>, Error> via SendPolicy. Prepared handling and encoding: [shared contract].” This is a prepared-request description, not a claim about later policy edits.

Call site (caller writes):

~~~rust
// Before
let response = api.response_stream(request).send().await?;
// After
let response = api.response_stream(request).send().await?;
~~~

SDK code that disappears: OpenAI's repeated generic header explanations at examples/openai-sdk/src/lib.rs:27-29, 39-40, 46-47 and 55-56: 9 comment lines, zero runtime lines. Keep the operation purposes, multipart boundary/provider facts and the declaration itself. Generator prose at crates/concord-macros/src/emit.rs:1211, 1326, 1334, 1453 and 1520-1522 is replaced by shared links; the long 1403-1452 explanations become compact fact rows. Those are generator changes, not 55 independently counted SDK savings. No construction wrapper disappears.

Hardest counterexample: Mastodon: its declared mastodon.invalid is a placeholder, not its deployed origin. Label the URL “declared/prepared base” and preserve its instance/advertised-host notes. This design cannot infer a runtime base fact the language cannot express. Kubernetes and MediaWiki keep explicit ordered query facts; shortening their docs does not change their ordering rules.

Simple control (Nager): Unchanged declaration and calls; shorter generated documentation. PokeAPI and Frankfurter also require no edits.

Charter check:

- C1: No new declaration words; documentation budgets are compiler-project CI policy.
- C2: Print existing provider facts; leave all send-policy decisions in Rust.
- C3: All three control declarations remain byte-for-byte unchanged.
- C4: Keep the complete declared URL and ordered query summary, with an explicit prepared-path label.
- C5: No new author diagnostics; doc tests identify the generated endpoint exceeding the budget.
- C6: Shared mechanics leave the caller path; preserve short operation-specific facts.
- C7: Keep prepare(), into_parts(), send_raw() and the caller's transport visible through the shared link.
- C8: Documentation-only change; no runtime actor or request change.
- C9: Required checks: word-count snapshots, rustdoc link checks, policy-versus-prepared result snapshots, unchanged raw-URL fixtures and secret canaries. Not run in this lane.

New concepts: None in the language or caller API; one shared documentation destination and a generator word-count check.

Breaking changes: Documentation snapshots/anchors may change; preserve useful existing anchors. No source, type or wire break.

Why it might be wrong:
The shared page can become an inconvenient second lookup for a simple question.
The 150-word generated budget alone does not constrain long SDK-authored explanations; report both rather than hiding one.

## H2. The declaration is the documentation, with its inherited context

Fact it expresses: An operation's wire contract is its declaration plus the enclosing facts it inherits.

Mechanism: Replace generated composition prose with a canonical declaration excerpt. Render the selected endpoint inside only the client/scope context that affects it; expand parameter groups at their written position or link to their definition. Keep explicit override/remove operations and their owner context. This is a source-shaped explanation, not H1's resolved fact table. Retain an independently composed method/URL line so the reader need not mentally concatenate a long tree.

Declaration (author writes), FRED, examples/fred-sdk/src/lib.rs:117-124, under client path fred and scope path series:

~~~rust
// Before
/// Retrieve the series metadata as XML.
/// The declared `Xml` format suggests `accept: text/xml`.
GET metadata() -> Metadata;
/// Retrieve the series metadata as JSON.
GET metadata_json() -> Metadata {
    query { "file_type" = "json"; }
    response concord::Json;
}

// After
/// Retrieve the series metadata as XML.
GET metadata() -> Metadata;
/// Retrieve the series metadata as JSON.
GET metadata_json() -> Metadata {
    query { "file_type" = "json"; }
    response concord::Json;
}
~~~

An illustrative generated excerpt for metadata_json preserves the actual inheritance:

~~~rust
pub client Fred(#[sensitive] query api_key: String) {
    base "https://api.stlouisfed.org";
    path "fred";
    response Xml;
    limit BODY_LIMIT;
    send Policy -> Result<_, Error>;
    scope series(query series_id: String) {
        path "series";
        GET metadata_json() -> Metadata {
            query { "file_type" = "json"; }
            response concord::Json;
        }
    }
}
~~~

It describes GET /fred/series, not /fred/series/metadata_json. Render private/custom format names as source with a reliable definition link where available; do not claim the procedural macro can evaluate an arbitrary Decode::ACCEPT associated constant. Secret initializers/literals must be withheld in rendered excerpts; a raw token dump is not acceptable.

Call site (caller writes):

~~~rust
// Before
let metadata = fred.series("GDP").metadata_json().send().await?;
// After
let metadata = fred.series("GDP").metadata_json().send().await?;
~~~

SDK code that disappears: Repeated format-to-Accept narration at examples/fred-sdk/src/lib.rs:79-82, 103, 118, 126 and 139: 8 comment lines; zero runtime lines. The CSV aggregation caveat at 137-138 stays. These comments are not classified as runtime glue by GLUE.draft.md. Replace the textual reconstruction in crates/concord-macros/src/emit.rs:1100-1213 and the prose assembly at 1548-1583 for this documentation mode with source/context rendering; do not remove the underlying composition model.

Hardest counterexample: MediaWiki: a bare search excerpt omits action=query, format=json and formatversion=2, so it is insufficient. Include the query scope at examples/mediawiki-action-api-sdk/src/lib.rs:42-44 and the endpoint inputs before list=. Dynamic continuation remains an opaque QueryPairs expansion with a link to Continuation, never an invented static list of keys. Kubernetes WatchOptions likewise must be expanded or directly reachable.

Simple control (Nager): Unchanged. Render its existing declaration; no new doc attribute, opt-in statement or required rewrite.

Charter check:

- C1: Source already consists of provider facts; no documentation directives are added.
- C2: The excerpt never fabricates behavior from a policy's name or output type.
- C3: Existing control declarations remain valid and unchanged.
- C4: Show inherited context and the independently resolved URL, not an isolated misleading fragment.
- C5: Excerpt/link errors point to author source; preserve source spans for subsequent diagnostics.
- C6: Remove paragraphs explaining syntax; keep a short excerpt and links for large groups.
- C7: One concise prepared/policy distinction links to all native exits.
- C8: Source rendering executes no initializer and builds no request.
- C9: Required checks: context/excerpt snapshots for FRED, MediaWiki and Kubernetes; secrets in initializers; cfg-hidden endpoints; agreement with prepared raw URLs. Not run here.

New concepts: No DSL syntax; a context-preserving excerpt renderer.

Breaking changes: Documentation layout and text; no Rust or wire changes. Source excerpts can expose private type names already present in signatures, so links must tolerate private modules.

Why it might be wrong:
Syntax may be less useful than a compact input/result table to an SDK caller unfamiliar with Concord.
Expanding a large parameter group can exceed the intended caller reading budget; linking must not conceal order-critical facts.

## H3. Name the stream reader, and have Rust check the name

Fact it expresses: The declared stream item and format produce a particular public reader type.

Mechanism: Allow an optional reader type assertion after a typed streaming output. The one-sentence rule is: “stream Item as Reader means this format's Incremental<Item>::Records is Reader.” Emit the named type in prepare/send/result-template signatures and check associated-type equality at the annotation. This solves a type presentation problem rather than changing prose organization.

Declaration (author writes), Mastodon, examples/mastodon-sdk/src/api.rs:83-89:

~~~rust
// Before
GET public_stream(
    /// `only_media`: only statuses with media attachments.
    query only_media?: bool,
) -> stream StreamEvent {
    path "api/v1/streaming/public";
    response PublicStreamFormat;
}

// After; import crate::streaming::PublicStream alongside the existing format/item imports.
GET public_stream(
    /// `only_media`: only statuses with media attachments.
    query only_media?: bool,
) -> stream StreamEvent as PublicStream {
    path "api/v1/streaming/public";
    response PublicStreamFormat;
}
~~~

The equality is grounded in examples/mastodon-sdk/src/streaming.rs:193-195: PublicStreamFormat already declares type Records = PublicStream. Apply the assertion to the other two stream signatures at api.rs:92 and 98 as well. The effective return type stays Result<PublicStream, Error> through the existing policy.

Call site (caller writes):

~~~rust
// Before
let mut stream = mastodon.public_stream(&host)?.send().await?;
// After
let mut stream = mastodon.public_stream(&host)?.send().await?;
~~~

Rustdoc now names PublicStream directly, instead of requiring the caller to trace Records<PublicStreamFormat, StreamEvent>. It must still show concord::Error on prepare() and the SDK Error on policy send().

SDK code that disappears: The five-line explanation that these three endpoint sends yield PublicStream at examples/mastodon-sdk/src/streaming.rs:211-215 can become one purpose line: roughly 4 comment lines saved, zero runtime lines. Kubernetes' analogous return-type explanation at examples/kubernetes-sdk/src/watch.rs:31-33 can become one line if its six declarations assert crate::watch::WatchStream<Pod/Deployment/Ingress>; about 2 more comment lines saved. Keep its WatchStream alias at watch.rs:294 and all reader behavior. Change the projection-only emission at crates/concord-macros/src/emit.rs:1023-1029 and 2006-2011; no reader implementation or forwarding wrapper disappears.

Hardest counterexample: Kubernetes: assert WatchStream<Pod> for watch_pods, not WatchEvent<Pod> or a reader with a different source type. A wrong reader must fail at “as WatchStream<Pod>.” watch=1 order and authored JSON Accept are untouched. OpenAI's untyped -> stream remains available and needs no reader assertion.

Simple control (Nager): Unchanged; finite endpoints gain no annotation. PokeAPI and Frankfurter also remain unchanged.

Charter check:

- C1: The added type asserts the typed response contract, like the existing output type; it does not select a generator mode. If that distinction is judged insufficient, reject this design.
- C2: The reader's implementation and every stream decision remain Rust.
- C3: Annotation is optional and confined to typed streams.
- C4: It cannot affect method, URL, headers or query composition.
- C5: Check Incremental<Item, Records = Reader> at the reader annotation, naming authored types.
- C6: One type-equality rule, not another explanation of stream projections.
- C7: The original format route, untyped Incoming route and native exits remain available.
- C8: No new adapter, conversion, task or additional body read.
- C9: Required checks: equality/mismatch compile tests, exact public signature snapshots and existing unread-head/terminal-stream tests. Not run here.

New concepts: Optional “as Reader” in typed stream output; no new runtime trait.

Breaking changes: Additive syntax. Old signatures remain accepted and denote the same types; a changed format reader now deliberately fails at an asserted contract.

Why it might be wrong:
It repeats the type association already written in the Rust Incremental implementation.
A rustdoc improvement may fix the presentation without adding any language; this is the least compelling H candidate.

# Family K — domain facts and composition

## K1. A literal path prefix that adds no navigation

Fact it expresses: A specified set of sibling operations shares the same literal URL prefix.

Mechanism: Add prefix "literal" { children }. Its only contribution is path text; it owns no values, setters, formats or policy. It creates no scope handle, generated type-name segment or operation-name segment. Restrict the first version to literal prefixes with no holes; values continue to belong to a real client or scope.

Declaration (author writes), Mastodon, excerpts at examples/mastodon-sdk/src/api.rs:51-53, 71-79:

~~~rust
// Before
GET public_timeline(..TimelineOptions) -> Vec<Status> {
    path "api/v1/timelines/public";
}
GET status(id: StatusId) -> Status {
    path "api/v1/statuses/{id}";
}
GET instance() -> InstanceInfo {
    path "api/v2/instance";
}

// After
prefix "api/v1" {
    GET public_timeline(..TimelineOptions) -> Vec<Status> {
        path "timelines/public";
    }
    GET status(id: StatusId) -> Status {
        path "statuses/{id}";
    }
}
GET instance() -> InstanceInfo {
    path "api/v2/instance";
}
~~~

Move hashtag_timeline and all three typed streams into the same v1 prefix, retaining their exact inputs/formats and relative suffixes. The v2 instance stays outside. Do not introduce mastodon.v1().public_timeline(): the version is a path fact, not an extra domain context a caller must navigate.

Call site (caller writes):

~~~rust
// Before
let page = mastodon.public_timeline().send().await?;
let instance = mastodon.instance().send().await?;
// After
let page = mastodon.public_timeline().send().await?;
let instance = mastodon.instance().send().await?;
~~~

SDK code that disappears: Six repeated full-prefix path statements at examples/mastodon-sdk/src/api.rs:52, 67, 72, 87, 93 and 99 are replaced with suffix statements and one shared prefix. Six copies of api/v1 become one: 5 repeated prefix literals disappear. Zero runtime-glue lines and zero path-statement lines disappear; a formatted group adds 2 delimiter/header lines. Report that cost honestly rather than counting shortened strings as removed functions. Mastodon's construction and streaming-host wrappers at src/lib.rs:124-150 and 178-239 stay.

Hardest counterexample: Mastodon itself: api/v2/instance remains visibly exceptional; there is no “everything is v1 unless guessed otherwise” rule. Kubernetes' existing core_v1/apps_v1 navigation scopes keep their domain meaning and need no conversion. Query order, including watch=1 and MediaWiki list=, is unchanged because prefix contributes no query entries.

Simple control (Nager): Unchanged. Its single api/v4 path needs no prefix group. PokeAPI's base prefix and Frankfurter's two explicit paths stay valid.

Charter check:

- C1: The group states shared provider URL text, with no surface-suppression directive.
- C2: No signing, host choice, token routing or version negotiation enters the block.
- C3: All old declarations remain valid; controls gain no lines.
- C4: Concatenate base path, ancestor paths, literal prefixes and endpoint suffix in visible tree order.
- C5: Reject leading slash, holes and invalid prefix text at the prefix literal; preserve child spans.
- C6: “Adds this path prefix to its children; adds no navigation.”
- C7: Native exits and the supplied reqwest client are unaffected.
- C8: Macro expansion only; operation identities and generated type names exclude the prefix group.
- C9: Required checks: all seven Mastodon routes and names before/after, Unicode/slash-hole encoding, mixed v1/v2, base replacement and prefix placement under scopes. Not run here.

New concepts: One restricted prefix block; no new caller type or trait.

Breaking changes: Additive syntax. No caller or wire break for the shown migration. Moving an endpoint into a prefix while forgetting to shorten its path produces a doubled route, so independent wire tests remain necessary.

Why it might be wrong:
It saves repeated text while slightly increasing raw declaration lines and adding a grouping concept.
If shared fixed queries and formats are subsequently added, it risks becoming a second, confusing kind of scope.

## K2. Fixed query pairs inside existing parameter groups — smaller than you think

Fact it expresses: A family of operations always sends the same query pair, at the stated position.

Mechanism: Extend parameter entries with query("key") = "literal". Unlike query watch: u8 = 1, this is a literal contribution, not a value: no field, argument, initializer or setter. Permit it in params groups and endpoint input lists. Expand it exactly where written, using existing conflict and first-appearance-slot rules. This supplies the missing order control without changing the ordering of any existing declaration.

Declaration (author writes), Kubernetes, examples/kubernetes-sdk/src/api.rs:79-89 and 109-114; author comments omitted:

~~~rust
// Before
params WatchOptions(
    query("labelSelector") label_selector?: LabelSelector,
    query("fieldSelector") field_selector?: FieldSelector,
    query("resourceVersion") resource_version?: ResourceVersion,
    query("resourceVersionMatch") resource_version_match?: ResourceVersionMatch,
    query("sendInitialEvents") send_initial_events?: bool,
    query("allowWatchBookmarks") allow_watch_bookmarks?: bool = true,
    query("timeoutSeconds") timeout_seconds?: u32,
);
GET watch_pods(..WatchOptions) -> stream WatchEvent<Pod> {
    path "pods";
    query { "watch" = "1"; }
    headers { ACCEPT = "application/json"; }
    response JsonStream;
}

// After
params WatchOptions(
    query("labelSelector") label_selector?: LabelSelector,
    query("fieldSelector") field_selector?: FieldSelector,
    query("resourceVersion") resource_version?: ResourceVersion,
    query("resourceVersionMatch") resource_version_match?: ResourceVersionMatch,
    query("sendInitialEvents") send_initial_events?: bool,
    query("allowWatchBookmarks") allow_watch_bookmarks?: bool = true,
    query("timeoutSeconds") timeout_seconds?: u32,
    query("watch") = "1",
);
GET watch_pods(..WatchOptions) -> stream WatchEvent<Pod> {
    path "pods";
    headers { ACCEPT = "application/json"; }
    response JsonStream;
}
~~~

Remove the watch block from all six watch endpoints. Putting the constant last is intentional: the default remains allowWatchBookmarks=true&watch=1. A provider or new operation that needs it first can write it first, explicitly; that is a deliberate byte change, not a new default.

Second witness: share MediaWiki's fixed revision selection without adding setters or navigation. Before, examples/mediawiki-action-api-sdk/src/lib.rs:56-71:

~~~rust
GET content(query pageids: u64) -> Envelope<ContentResults> {
    query {
        "prop" = "revisions"; "rvprop" = "ids|content";
        "rvslots" = "main"; "rvlimit" = "1";
    }
}
GET contents(
    query pageids: PageIds,
    query(..) continuation?: Continuation,
) -> Envelope<ContentResults> {
    query { "prop" = "revisions"; "rvprop" = "ids|content"; "rvslots" = "main"; }
    limit BATCH_BODY_LIMIT;
}

// After: group is declared on the client, before scope query.
params RevisionQuery(
    query("prop") = "revisions",
    query("rvprop") = "ids|content",
    query("rvslots") = "main",
);
// The following endpoints remain inside scope query.
GET content(query pageids: u64, ..RevisionQuery) -> Envelope<ContentResults> {
    query { "rvlimit" = "1"; }
}
GET contents(
    query pageids: PageIds,
    query(..) continuation?: Continuation,
    ..RevisionQuery
) -> Envelope<ContentResults> {
    limit BATCH_BODY_LIMIT;
}
~~~

The single-ID-only rvlimit remains outside the group. Existing mutable group members still generate their setter trait; constant-only groups need no empty trait.

Call site (caller writes):

~~~rust
// Before
let watch = kube.core_v1().watch_pods().set_timeout_seconds(30).prepare()?;
let pages = wiki.query().contents(ids).send().await?;
// After
let watch = kube.core_v1().watch_pods().set_timeout_seconds(30).prepare()?;
let pages = wiki.query().contents(ids).send().await?;
~~~

SDK code that disappears: examples/kubernetes-sdk/src/api.rs:111, 135, 164, 182, 218 and 236: 6 fixed-query lines disappear, replaced by 1 group entry, net 5 declaration lines saved. MediaWiki's duplicate trio at src/lib.rs:57-60 and 70 is replaced by one shared group plus the separate rvlimit; 3 duplicated key/value facts disappear, but the shown formatting may add a line overall. Zero runtime glue disappears. No mutable watch setter is accidentally introduced.

Hardest counterexample: MediaWiki search: its current list=search must remain after srsearch, srlimit and continuation. The equivalent explicit spelling puts query("list") = "search" and query("srprop") = "" after those entries; moving them first is a declared wire change. Group expansion must preserve QueryPairs as a position-bearing entry, not sort it around literals. Kubernetes retains its exact tested watch tail.

Simple control (Nager): Unchanged; no parameter groups are required. PokeAPI and Frankfurter also retain their existing query/default forms.

Charter check:

- C1: A literal query pair is a provider fact, not a command to remove a setter.
- C2: The compiler repeats facts; paging/retry/validation decisions stay Rust.
- C3: All existing input and query-block forms remain valid and unchanged.
- C4: Constant entries occupy the position at which they are written/spliced; no new global sort.
- C5: Duplicate keys, malformed literals and conflicting inherited contributions point to the literal/group inclusion.
- C6: “A nameless query literal is always sent and has no setter.”
- C7: Native request editing and the existing QueryPairs escape remain available.
- C8: Literal expansion has no initialization effects or runtime actors.
- C9: Required checks: six raw watch targets, MediaWiki list/continuation order, absence of set_watch, fixed-only groups, group conflict spans and sensitive-key redaction. Not run here.

New concepts: A nameless fixed query entry in the existing parameter grammar; no new trait.

Breaking changes: Additive syntax only. Keep existing query blocks and their wire rules. A mixed group's expanded entry positions must not reorder its required Rust arguments.

Why it might be wrong:
Mixing values and constants in params makes its meaning broader than “a set of setters.”
It does not simplify inherited append/override slot rules; those should not be quietly changed as part of this feature.

## K3. A typed mirror of the request facts, usable by ordinary Rust

Fact it expresses: Declared methods, literal paths, path templates and named query keys form a known request contract.

Mechanism: Generate read-only RequestFacts and QueryKeys accessors from the same resolved model used to prepare requests. Each concrete endpoint has request_facts(); the client has declared_query_keys(), the union of statically named keys across its declaration. No extra operations or “reflect” statement. This is typed access to existing facts, not runtime inference or a middleware registry.

Keep Operation's current value-free identity/Debug behavior. Do not turn its formatting into a URL logger. A path fact distinguishes Literal from Template and returns no literal for a route with holes. Facts contain no client values or evaluated defaults.

Declaration (author writes), MediaWiki, examples/mediawiki-action-api-sdk/src/lib.rs:47-53:

~~~rust
// Before
GET search(
    query srsearch: String,
    query srlimit: u16 = 10,
    query(..) continuation?: Continuation,
) -> Envelope<SearchResults> {
    query { "list" = "search"; "srprop" = ""; }
}

// After: identical declaration; facts are generated from it and its query scope.
GET search(
    query srsearch: String,
    query srlimit: u16 = 10,
    query(..) continuation?: Continuation,
) -> Envelope<SearchResults> {
    query { "list" = "search"; "srprop" = ""; }
}
~~~

Rust fact consumer (SDK writes), examples/mediawiki-action-api-sdk/src/models.rs:33-55:

~~~rust
// Before: inside Continuation::deserialize.
if key.is_empty()
    || matches!(
        key.as_str(),
        "action"
            | "format"
            | "formatversion"
            | "list"
            | "prop"
            | "srsearch"
            | "srlimit"
            | "srprop"
            | "pageids"
            | "rvprop"
            | "rvslots"
            | "rvlimit"
            | "cmtitle"
            | "cmlimit"
    )
{
    return Err(D::Error::custom(
        "continuation collides with a fixed query input",
    ));
}

// After: same provider rule, including the old across-endpoint reservation.
if key.is_empty() || crate::MediaWiki::declared_query_keys().contains(key.as_str()) {
    return Err(D::Error::custom(
        "continuation collides with a fixed query input",
    ));
}
~~~

The client-wide union is deliberate: an endpoint-only set would weaken the current rejection of keys belonging to sibling queries. It includes named inputs even when optional/overridden/removed, because this is the declared vocabulary, not just surviving pairs. Dynamic QueryPairs keys remain unknown; they are not fabricated. The SDK still decides to reject collisions; Concord does not impose that policy on arbitrary QueryPairs users.

Second witness, unchanged OpenAI declaration at examples/openai-sdk/src/lib.rs:31-37:

~~~rust
base "https://api.openai.com";
path "v1";
send SendPolicy -> Result<Response<_>, Error>;
request Json;
response Json;
/// Create a response.
POST response(body request: ResponseRequest) -> ResponseObject { path "responses"; }
~~~

Before helper fields at src/lib.rs:109 and 158-159:

~~~rust
// BatchRequest::responses:
endpoint: "/v1/responses".into(),
// BatchInput::response:
method: "POST".into(),
url: "/v1/responses".into(),
~~~

After, in the same Rust helpers:

~~~rust
// A small ordinary Rust helper can share this extraction between both constructors.
let route = OpenAiApiResponse::request_facts();
let path = route.literal_path().expect("the declared response route is literal");
// BatchRequest::responses:
endpoint: path.into(),
// BatchInput::response:
method: route.method().as_str().into(),
url: path.into(),
~~~

literal_path() includes declared base-path and client/scope path text, starts with '/', and preserves a declared trailing slash. It describes the declared provider route, not a rebased fixture URL or policy-edited request. For a templated route, use prepare() with real inputs rather than interpolating it by hand. Operation::method() already exists (concord-core/src/response.rs:68-75); do not claim method access alone as a new capability.

Call site (caller writes): public SDK callers remain unchanged.

~~~rust
// Before
let response = wiki.query().search("Rust").send().await?;
// After
let response = wiki.query().search("Rust").send().await?;
~~~

SDK code that disappears: The 14-key hand-maintained list and its matches! scaffolding at examples/mediawiki-action-api-sdk/src/models.rs:35-51, 17 physical lines replaced by a generated-set lookup; roughly 16 lines of table boilerplate saved, with exact formatting determining the total. Keep empty-key, generic-continue and string/integer checks. OpenAI's duplicate route literals at src/lib.rs:109 and 159 disappear; those two constructor lines are replaced, not the constructors. Extraction may add a few lines, so no net OpenAI LOC saving is claimed.

Hardest counterexample: Mastodon: facts identify separate /api/v1 and /api/v2 routes; no “version of client” is guessed. Host discovery and token/host binding stay in Rust and its wrapper. Kubernetes/MediaWiki facts must carry ordered query contributions separately from membership: a QueryKeys set must never be used to serialize or document query order.

Simple control (Nager): Unchanged declaration and call sites; additive generated accessors only. No new input or required import for any control.

Charter check:

- C1: No new DSL directives; accessors expose facts that are already written.
- C2: This directly removes restated facts while keeping collision rules and batch construction in Rust.
- C3: Control declarations are unchanged; measure generated-code cost separately.
- C4: Literal/template distinction is typed; no name-based path, version or key inference.
- C5: Fact-generation failures retain original declaration spans; a dynamic path is never passed off as a literal.
- C6: “Read the declared route/keys; use prepare() for an actual request.”
- C7: RequestFacts is advisory description; native exits and custom policies remain first-class.
- C8: Immutable generated data, no global registry, initializer evaluation or network activity.
- C9: Required checks: exact old MediaWiki reserved set, omission mutation after adding a new named key, cfg policy, literal versus templated paths, base paths/trailing slashes, fact/prepare agreement and value-free Debug canaries. Not run here.

New concepts: RequestFacts, QueryKeys, a literal/template path distinction; inherent request_facts() and declared_query_keys() accessors. Reserve/check these generated method names explicitly.

Breaking changes: New accessor names can collide with existing endpoint names; diagnose both declarations or choose a namespaced facts API before stabilizing. No intended caller or wire break. Specify whether cfg-disabled keys remain in the declared vocabulary; conservatively include them to match static sensitivity classification, while endpoint enumeration follows compiled cfgs.

Why it might be wrong:
A full request-description schema could become a second API to stabilize; begin with literal paths and named keys, not a universal schema.
OpenAI batch helpers must continue to choose their supported route intentionally; a future streaming/parameter change cannot make every operation batchable automatically.

Cross-family effect: This is the strongest shared mechanism in this lane. The same resolved facts support H documentation/index generation, K path/key reuse, E operation classification inputs, and F continuation validation. It does not itself generate retry or paging behavior, unwrap results, or remove construction wrappers.

# Family L — syntax and conventions

## L1. Explicit inline path, with the old block still available — smaller than you think

Fact it expresses: This operation's relative path is the stated literal template.

Mechanism: Add one shorthand, GET name(args) -> Type at "path";, desugaring exactly to a one-path body. Keep the block form for all other facts and keep omitted path semantics unchanged. This is deliberately only parsing sugar; do not sell it as removal of runtime glue.

Declaration (author writes), Mastodon, examples/mastodon-sdk/src/api.rs:51-53 and 71-79:

~~~rust
// Before
GET public_timeline(..TimelineOptions) -> Vec<Status> {
    path "api/v1/timelines/public";
}
GET status(id: StatusId) -> Status {
    path "api/v1/statuses/{id}";
}
GET instance() -> InstanceInfo {
    path "api/v2/instance";
}

// After
GET public_timeline(..TimelineOptions) -> Vec<Status> at "api/v1/timelines/public";
GET status(id: StatusId) -> Status at "api/v1/statuses/{id}";
GET instance() -> InstanceInfo at "api/v2/instance";
~~~

No parentheses are removed and no endpoint name is interpreted as path text. FRED can similarly shorten its search and observations bodies, while metadata() -> Metadata; continues to append no segment. No-output endpoints may use the same at suffix without gaining typed-output semantics.

Call site (caller writes):

~~~rust
// Before
let status = mastodon.status(id).send().await?;
// After
let status = mastodon.status(id).send().await?;
~~~

SDK code that disappears: The three 3-line blocks at examples/mastodon-sdk/src/api.rs:51-53, 71-73 and 77-79 become three declarations, approximately 6 physical lines saved. FRED path-only blocks at examples/fred-sdk/src/lib.rs:104-106 and 127-129 save approximately 4 lines when kept on one line. The path facts survive intact; zero runtime-glue lines disappear. OpenAI already places several one-path blocks on one physical line, so it saves tokens there, not lines. Long signatures can still wrap, reducing apparent LOC savings.

Hardest counterexample: C4/Mastodon: v1 and v2 stay explicit in each template, with the same encoding as path. Kubernetes watch_pods and MediaWiki search retain blocks because they have other facts; inline paths do not suppress query facts. PokeAPI's trailing slash must remain literal.

Simple control (Nager): Its existing block remains unchanged and valid. No mandatory style rewrite of PokeAPI or Frankfurter.

Charter check:

- C1: at states the provider path, not a rendering preference.
- C2: No execution logic; lower to the current path statement before semantic analysis.
- C3: Existing syntax is retained; all controls remain unchanged.
- C4: URL text is fully explicit; no pluralization, casing or name inference.
- C5: Invalid templates and unknown holes point to the inline literal, not a lowered synthetic body.
- C6: “at is the short form of a body containing only path.”
- C7: Generated types, traits and native exits are identical.
- C8: Exactly the old resolved model and preparation code.
- C9: Required checks: block/inline equivalence of generated API and prepared bytes, typed/no-output/stream cases, trailing slash, holes and error spans. Not run here.

New concepts: One contextual at suffix; no type or trait.

Breaking changes: Additive syntax; ensure a Rust type/path named at is not misparsed. No source, type or wire break for migration.

Why it might be wrong:
It makes two spellings for the same fact and saves little in already compact one-line code.
If H is the real readability problem, a parser feature may be unnecessary work.

## L2. An explicit name-to-path convention, never a global default

Fact it expresses: The provider path segment is exactly the endpoint's written name.

Mechanism: Permit GET @name(args) -> Type;. The @ is a visible route marker: it means one literal path segment equal to name, with no case conversion, pluralization, underscore conversion or trailing slash. It is a different tradeoff from L1: the path spelling is shared with the Rust method name, rather than repeated as a string. No-marker declarations keep all current semantics.

Declaration (author writes), Kubernetes, examples/kubernetes-sdk/src/api.rs:98-105 and 116-120:

~~~rust
// Before
GET namespaces(..ListOptions) -> NamespaceList { path "namespaces"; }
GET pods(..ListOptions) -> PodList { path "pods"; }
GET services(..ListOptions) -> ServiceList { path "services"; }
GET config_maps(..ListOptions) -> ConfigMapList { path "configmaps"; }

// After, in the same core_v1 scope
GET @namespaces(..ListOptions) -> NamespaceList;
GET @pods(..ListOptions) -> PodList;
GET @services(..ListOptions) -> ServiceList;
GET config_maps(..ListOptions) -> ConfigMapList { path "configmaps"; }
~~~

config_maps is deliberately unchanged: its wire spelling is configmaps. version() at api.rs:92 also stays explicit because its path ends in '/'. A raw Rust identifier r#type maps to the literal type; document and test that single escaping rule. Reject @ plus an explicit path instead of inventing precedence.

Call site (caller writes):

~~~rust
// Before
let pods = kube.core_v1().pods().prepare()?;
// After
let pods = kube.core_v1().pods().prepare()?;
~~~

SDK code that disappears: The duplicate path-name strings/statements at examples/kubernetes-sdk/src/api.rs:99, 105 and 117 disappear into their endpoint names. Because these are already one-line declarations, 0 physical lines and 0 runtime-glue lines are saved in this excerpt; 3 redundant literal path facts are removed. The same applies to the matching path-name endpoints at 127, 141, 159, 174, 193, 201, 213 and 228. Do not count config_maps or version among matches.

Hardest counterexample: FRED: current GET metadata() -> Metadata; must still target /fred/series, not /fred/series/metadata. Only the explicit @ opts into a segment. Mastodon has no matching one-segment names for its full routes, so it stays explicit or uses K1 first. Kubernetes watch_pods does not imply /watch_pods; it retains its pods path and fixed watch query.

Simple control (Nager): Unchanged and cannot sensibly use the shorthand for its country/year route. PokeAPI detail/list and Frankfurter pair routes likewise keep explicit templates.

Charter check:

- C1: The marker states exact equality between the operation label and a provider path segment; no generator control statement.
- C2: No behavioral convention follows from a name.
- C3: No-marker declarations and all controls keep their meaning and shape.
- C4: Equality is explicitly marked; no ambient name-to-path default is permitted.
- C5: Reject a second path at its source and diagnose invalid segment spellings at @name.
- C6: “@name contributes the literal path segment name.”
- C7: Native exits and explicit path syntax remain available.
- C8: A literal expansion; no runtime decisions.
- C9: Required checks: marker/no-marker contrast, FRED metadata regression, config_maps mismatch control, raw identifiers, case/underscore spelling, trailing slash and generated-name equivalence. Not run here.

New concepts: One route sigil, @; no new type or trait.

Breaking changes: Additive syntax only. Subsequently renaming an @ endpoint changes both its Rust method and its URL; migration tooling must materialize the old literal path when only a Rust rename is intended.

Why it might be wrong:
It couples two names that maintainers may later want to change independently, while saving only a short string.
The extra sigil may cost more learning than the 42 exact-name paths in the family brief justify; prefer L1 unless evidence shows a caller benefit.

## L3. One wire route with several explicitly named representations

Fact it expresses: One GET route accepts the same inputs but has several statically named query/format/result combinations.

Mechanism: Permit a route-first GET family with common inputs and explicitly named result rows. Each row becomes an ordinary endpoint in the enclosing scope; the family is not a scope or a runtime format switch. A row inherits common method/path/inputs and then has the usual query/response facts. Restrict the first design to identical inputs, no new values at family level beyond those inputs, and no nested navigation.

Declaration (author writes), FRED, examples/fred-sdk/src/lib.rs:127-144, omitting comments in the before excerpt:

~~~rust
// Before, inside scope series(query series_id: String), path "series".
GET observations(query limit: ObservationLimit = ObservationLimit::default(), ..OffsetOptions, ..ObservationOptions) -> Observations {
    path "observations";
}
GET observations_json(query limit: ObservationLimit = ObservationLimit::default(), ..OffsetOptions, ..ObservationOptions) -> Observations {
    path "observations";
    query { "file_type" = "json"; }
    response concord::Json;
}
GET export_csv(query limit: ObservationLimit = ObservationLimit::default(), ..OffsetOptions, ..ObservationOptions) -> CsvExport {
    path "observations";
    query { "file_type" = "csv"; }
    response ZippedCsv;
}

// After, in the same series scope; default response Xml remains inherited.
GET "observations" (
    query limit: ObservationLimit = ObservationLimit::default(),
    ..OffsetOptions, ..ObservationOptions
) {
    observations -> Observations;
    observations_json -> Observations {
        query { "file_type" = "json"; }
        response concord::Json;
    }
    /// Observation controls are forwarded; FRED documents aggregation only for XML/JSON.
    export_csv -> CsvExport {
        query { "file_type" = "csv"; }
        response ZippedCsv;
    }
}
~~~

The row names are the exact generated method names, not inferred from format type names. Xml is a custom Rust format and has no privileged syntax. The CSV row continues to return verified ZIP-export bytes through ZippedCsv, not a newly invented Csv decoder. The same table idea can express search/search_json, but this witness alone does not justify a universal representation framework.

Call site (caller writes):

~~~rust
// Before
let xml = fred.series("GDP").observations().send().await?;
let json = fred.series("GDP").observations_json().send().await?;
let csv = fred.series("GDP").export_csv().send().await?;
// After
let xml = fred.series("GDP").observations().send().await?;
let json = fred.series("GDP").observations_json().send().await?;
let csv = fred.series("GDP").export_csv().send().await?;
~~~

SDK code that disappears: The three repeated input lists at examples/fred-sdk/src/lib.rs:127, 131 and 140 become one; the three identical path statements at 128, 132 and 141 become one route literal. This removes 2 duplicate input-list facts and 2 duplicate path facts, not runtime code. In the shown, comfortably wrapped syntax, total physical lines may grow; no net LOC saving is claimed. The format/query pairings at 133-134 and 142-143 remain explicit. ObservationOptions, offset projection, codecs, Policy and limits all stay. This directly addresses FRED FRICTION.md F5 and MAINTENANCE.md:150-156 without a runtime format value.

Hardest counterexample: Mastodon: v1 timelines and v2 instance are different routes, not alternate representations of one route, so this construct must not combine them. Kubernetes list and watch differ in output mode and inputs; keep them independent until identical-input evidence justifies a family. MediaWiki content/contents have different pageids types and rvlimit semantics, so they also stay independent. Those exclusions are a strength of the restriction, not a claim to remove all repeated declarations.

Simple control (Nager): Unchanged; a single representation requires no family. PokeAPI and Frankfurter also retain their original declarations.

Charter check:

- C1: Common method/path/inputs and each representation's query/format/output are provider contract facts.
- C2: No runtime branch, negotiation algorithm or decoding policy is expressed.
- C3: Existing single-endpoint forms remain valid; controls gain nothing.
- C4: Every method name and representation query is explicit; no suffix or format-name inference.
- C5: Attach conflicts and output/format mismatches to the specific row; duplicated names name both rows.
- C6: “Each named row is an endpoint with this method, path and inputs.” Reject further template features if they need more explanation.
- C7: Every row implements the existing endpoint/send traits and exposes all native exits.
- C8: Expand rows before semantic analysis; defaults still evaluate once per endpoint construction, never once for the family.
- C9: Required checks: all three FRED prepared targets, query order, defaults/setters, concrete types and custom format head checks; generated identities/cfg rows; no-output/stream rows only if explicitly supported. Not run here.

New concepts: A route-first family and explicit endpoint rows; no runtime enum or new transport trait.

Breaking changes: Additive grammar. The migration must retain FredSeriesObservations, FredSeriesObservationsJson and FredSeriesExportCsv identities/types and the existing group setter traits. Do not change send Policy -> Result<_, Error> semantics.

Why it might be wrong:
FRED is currently the strongest evidence, and its own friction log cautions that separate methods are acceptable.
The family can reduce repeated facts while increasing syntax and lines; reject it if L1 plus ordinary parameter groups is clearer.

# Selection and boundaries

First candidates to prototype are H1, K2 and K3. H1 removes the generated reading tax without a language change; K2 removes the repeated immutable watch fact while preserving exact order; K3 removes a real hand-maintained key table and supplies facts to multiple other families. H2 is an alternative presentation to compare with H1, not a second additive documentation layer. K1 and L1 are modest syntax candidates. H3, L2 and L3 carry enough new language or duplication risk to need stronger evidence.

Do not apply the seed “query follows declaration order everywhere” as a casual global semantic change. Within an owner, current direct parameters already precede later query blocks. Across owners, inherited first-appearance slots intentionally group appended occurrences. Replacing them with a contribution stream changes URLs such as tag=a&tag=b&tag=c&v=2&limit=10 into tag=a&tag=b&v=2&limit=10&tag=c (AUTHOR_GUIDE.md §4.7). That is a separate wire-breaking proposal, not a documentation cleanup. K2 gives explicit placement for new fixed facts while preserving the current language.

Likewise, do not infer Result<_, Error> merely because a policy is called Policy or lives beside an SDK Error. World Bank SendWith<WireResponse> returns IndicatorPage (src/lib.rs:205-207), MediaWiki SendWith<Envelope<T>> returns QueryResponse<T>, and OpenAI returns Response<T>. Existing result templates can only be shortened when the asserted equality actually holds. Whole-envelope-versus-inner-value binding belongs to a separately designed result contract (families A/B), not an L convention guessed by the macro.

None of these proposals resolves the reverted controls/construction problem. Mastodon's private generated client, token/instance binding, discovery checks and wrapper methods remain necessary until a provider-fact construction design closes that gap. No documentation, naming or prefix proposal may claim their lines as savings.
