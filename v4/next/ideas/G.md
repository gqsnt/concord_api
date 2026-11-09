# G. Facades and naming — merged ideas
Sources: CG-claude.md (§ G-A to G-D, and § Seeds rejected in G), CG-codex.md (§ G1 to G3; the accessor part of § C1). 7 lane designs, plus 3 seeds the claude lane rejected → 8 entries: 4 KEEP, 3 KILL, 1 MERGED.

The forwarder census by cause (CG-claude.md § G, from `classify.py` at `ba10189b`) adds up to the inventory's 829
forwarding lines: construction 123, result shape 222, paging 122, validation 195 (≈40 of them actual
checks), naming 1, provider behavior 166. So naming glue is almost nil. Most forwarders go with
families C, A, B and F, and the G ideas below are what remains.

## G1. The generated client is the SDK: read accessors — KEEP (rank 1)
Sources: CG-claude.md § G-A; CG-codex.md § C1 (typed `context()` view, `http()`, explicit `*_unredacted()`); G8 merged here.
Fact: none new. This is typed access to declared values (charter C2: "behavior in Rust, with typed access to the facts").
Mechanism: the generator adds:
- a getter per client and scope value (`&T`, or `Option<&T>` for `x?`);
- `http()` and `base_url()` on clients, `client()` on scopes.

Today a client gets only `new`, `with_base_url`, setters and navigation (`concord-macros/src/emit.rs:1878-1911`, checked). Getters of sensitive values are either `pub(crate)` (claude) or explicitly named `x_unredacted()` (codex); the owner picks one.
Declaration: unchanged. Generated:
```rust
impl ShopifyAdmin { pub fn version(&self) -> &ApiVersion; pub fn http(&self) -> &reqwest::Client; }
impl ShopifyAdminShop { pub fn shop(&self) -> &ShopDomain; pub(crate) fn access_token(&self) -> &String; }
// before (shopify context.rs:64-75): a wrapper keeps a second `version` in step with set_version by hand
```
Call site:
```rust
let v = sdk.version();        // before: on a hand-written wrapper
let v = admin.version();      // after: on the generated handle; also admin.http(), s3.endpoint()
```
Glue removed: ≈70 lines directly (lane). Checked: 8 wrapper structs keep their own `http: reqwest::Client`: Shopify `context.rs:31` and `:114`, Salesforce `context.rs:49`, GitHub `context.rs:107`, HubSpot `lib.rs:383`, Strava `auth.rs:106`, Registry `client.rs:55` and PayPal `lib.rs:202`. With C1 and C2 it enables the wrapper deletions counted there, and the pure synonyms of G8. Not additive with C2.
Counterexample: secrets. A public getter would let any holder of a handle read the token back, hence `pub(crate)` or an explicit unredacted name. A getter can collide with a child navigation method of the same name; Concord already reports such collisions at the declaration (AUTHOR_GUIDE §9).
Simple control: declaration unchanged. The generated client gains `http()` and `base_url()`.
New concepts: value getters, `client()`, `http()`, `base_url()`.  Breaking: a value named like a child scope or endpoint of the same handle stops compiling (not seen in the witnesses; the 55 SDKs need a scan).
Shares mechanism with: C1 and C2 (with them, a wrapper has nothing left to hold), E and F (policies and pagers read `client.version()`), K2 (typed facts).
Screen:
- C1: no words added.
- C2: typed access to the facts.
- C3: no declaration change.
- C4: a getter has the value's name.
- C7: `http()` makes stepping out easier.
- C9: compile-fail test that a downstream crate cannot read a sensitive getter.

## G2. Recognize returned URLs as declared endpoints (`E::from_url`) — KEEP (rank 2)
Sources: CG-claude.md § G-C.
Fact: the URL shape the declaration already states, read backwards. A server-returned link is trustworthy when the declared endpoint could have built it from this client.
Mechanism: each endpoint type gets `from_url(&client, url) -> Result<Self, concord::UrlMismatch>`. It accepts a URL that has:
- the same origin and base path;
- client values equal to the client's;
- scope and endpoint values that parse with `FromStr`;
- only declared query keys;
- a canonical path.

Invariant: `from_url(c, u)?.prepare()` rebuilds `u` up to query-key order. Relative links are resolved against the base.
Declaration: unchanged. SDK before (`pokeapi-sdk/src/lib.rs:213-229`) → after: nothing.
```rust
pub async fn follow(&self, next: &str) -> Result<Response<PokemonPage>, Error> {
    let list = self.list().prepare()?;
    checked_url(list.request().url(), next, false)?;   // + checked_url, UrlRejection
    finish(list.follow(next)?.receive().await?).await
}
```
Call site:
```rust
let page = api.follow(next).await?;                                 // before
let page = PokeApiList::from_url(&api, next)?.send().await?;         // after
let more = SalesforceApiQueryMoreAccounts::from_url(&api, raw)?;    // replaces ctx.query_link(raw)
```
Glue removed: the lane estimates ≈350 lines. Checked (non-blank):
- Salesforce `context.rs:134-390` (249, all returned-link validation; ≈30 stay for the `resultpages`/`resultPages` discrepancy SF-D2 and record types) and `error.rs:490-600` (100 → ≈35).
- PokeAPI `lib.rs:117-155` (37) and `207-268` (60).
- Registry `client.rs:85-114` (27).

That is ≈470 lines checked, ≈400 after what stays. The lane's ≈350 is close and slightly conservative. Also relevant but not counted: friction C6 (Graph and Snowflake: `Prepared::follow` checks only the origin) and C4 (B2, GitHub, GitLab, Shopify address server-issued URLs by hand).
Counterexample: PokeAPI promises to follow `next` "exactly, retaining its query parameters and their order" (`lib.rs:208`). `from_url` rebuilds keys in declared order and refuses keys the endpoint does not declare: the same pairs, different bytes, unless the declaration lists keys in the server's order. Links the declaration does not model keep `Prepared::follow`.
Simple control: PokeAPI (a control) loses ≈95 lines. No control declaration changes.
New concepts: `Endpoint::from_url`, `concord::UrlMismatch`, and a `FromStr` bound on the path and query value types that take part.  Breaking: none (additive). It adds one function per endpoint, which counts against H1's word budget.
Shares mechanism with: F (link paging), K2 (`Route::matches` is its path-only subset), D (the URL never appears in an error).
Screen:
- C4: one stated round-trip invariant. The query-order rebuild must be shown on the page, or it surprises (PokeAPI).
- C5: `UrlMismatch` names the rule and the value, never the URL.
- C6: the checks fit in one short list.
- C7: `follow` stays.
- C8: no I/O.
- C9: property test of the round trip, plus a canary test.

## G3. Concord's build refusals are readable (`Error::invalid_value()`) — KEEP (rank 3)
Sources: CG-claude.md § G-B.
Fact: none new. Concord already refuses an empty, `.` or `..` path segment (`concord-core/src/__private.rs:420-425`, checked), but the value and the rule are not public, so SDKs check the same rule first to get their own error.
Mechanism: `concord::Error::invalid_value() -> Option<InvalidValue>`, with `what()` ("path segment `{id}`") and `rule() -> Rule` (`#[non_exhaustive]`). The SDK maps it once in its existing `From<concord::Error>`.
Declaration: unchanged. SDK before (`openai-sdk/src/lib.rs:396-401`) → after:
```rust
pub async fn file(&self, id: &str) -> Result<Response<FileObject>, Error> {
    if id.is_empty() { return Err(Error::Invalid("empty file ID")); }
    self.api.file(id).send().await
}
// after: the forwarder goes; in the existing From<concord::Error>:
if let Some(v) = error.invalid_value() { return Self::Invalid(v.what()); }
```
Call site: `openai.file(id).await?` → `openai.file(id).send().await?` (an empty id fails before anything is sent).
Glue removed: the lane says ≈50 lines in 5 SDKs; corrected to ≈30 lines in 4. Checked: OpenAI 9 check lines (`lib.rs:397-417`; ≈12 forwarder lines go too once family B keeps OpenAI's result shape), Drive `client.rs:205-207` (3), HubSpot (3), Linear (3). Twilio's `EmptySid` (`lib.rs:139-150` and 4 error sites) does not restate Concord's rule: its hole sits inside `"Messages/{sid}.json"` (`lib.rs:43`), and Concord checks only whole segments (friction K7). It goes only if Concord also refuses an empty hole inside a templated segment.
Counterexample: checks stricter than Concord's (OpenAI's filename rules, Elasticsearch's `IndexName::new`) stay in SDK types.
Simple control: unchanged.
New concepts: `Error::invalid_value()`, `InvalidValue`, `Rule`.  Breaking: none (additive). It freezes Concord's build rules as public API.
Shares mechanism with: A (error variants that exist only for these checks, such as Twilio `EmptySid` and OpenAI `Invalid("empty … ID")`); G5 (the killed alternative).
Screen:
- C1: no words added.
- C5: the error names the user's value, never internals.
- C6: one accessor and one small enum.
- C9: one test per rule.

## G4. Nest by context, not by URL (no Concord change) — KEEP (rank 4)
Sources: CG-claude.md § G-D.
Fact: one namespace is the context of every API group's namespaced resources (charter idea 1: a scope is a context).
Mechanism: none in Concord. A pathless context scope reads the outer value with the existing `{scope.value}` reference (AUTHOR_GUIDE §4.2). This is an SDK migration; it is in the gallery because it answers K4 (path blocks) for Kubernetes without new syntax.
Declaration: `kubernetes-sdk/src/api.rs:95-125`.
```rust
// before: scope core_v1 { path "api/v1"; scope namespaced(namespace: NamespaceName) { … } }, ×4 groups
scope namespace(namespace: NamespaceName) {
    scope core { path "api/v1/namespaces/{namespace.namespace}";
        GET pods(..ListOptions) -> PodList { path "pods"; } /* … */ }
    scope apps { path "apis/apps/v1/namespaces/{namespace.namespace}"; /* … */ }
}
```
Call site: `ctx.namespace(ns).core().pods()` → `kube.namespace(ns).core().pods()`. The shape is the same, and the handles are now generated.
Glue removed: ≈48 lines, checked: `kubernetes-sdk/src/context.rs:109-152` (38, `NamespaceContext` and its `Debug`) and `88-97` (10). With C2 and G1 the rest of `context.rs:22-107` (76) can go too (lane).
Counterexample: each group prefix is now written twice (cluster-wide and namespaced). Kubernetes FRICTION §3 calls today's layout "mostly intentional".
Simple control: unchanged.
New concepts: none.  Breaking: Kubernetes callers only (`core_v1().namespaced(ns)` → `namespace(ns).core()`). Both trees can coexist during a migration.
Shares mechanism with: K4 (the killed path blocks targeted the same wrapper).
Screen:
- C1: the scope states a provider fact.
- C3: no change to the controls.
- C4: every path stays visible on its scope.
- C6: one guide paragraph ("nest by context; reach an outer value with `<scope>.x`").

## G5. Validated inputs as declared types (SDK newtypes) — KILL (only moves glue)
Sources: CG-codex.md § G2; IDEAS.md G seed 2.
Design: Drive `GET get(id: FileId, query fields: FileFields)` and OpenAI `file(id: FileId)` move the check from the forwarder into a newtype constructor. Codex counts 34 gross wrapper lines removed and newtypes added, and says "net source lines may grow" (checked: Drive `client.rs:203-224`). Where the check restates Concord's own segment rule, G3 deletes it instead of moving it. Stricter checks are already SDK types.

## G6. Generic endpoint outputs (`GET records<R>(…) -> QueryResponse<R>`) — KILL (removes little for a large new concept)
Sources: CG-codex.md § G3.
Design: endpoint type parameters. It removes Salesforce's first/more dispatch (`query.rs:65-83` and three 12-line impls, 54 non-blank lines, checked) and 4 declarations, in 1 of 55 SDKs. Endpoint type parameters change generated type names, operation identity, `Clone` bounds and docs (codex: a "larger compiler surface"). Shopify's one-endpoint-per-GraphQL-operation layout would be a second witness only with generic bodies, which the design excludes.

## G7. Naming in the declaration: aliases, tree-shaped names, re-export conventions — KILL
Sources: CG-claude.md § Seeds rejected in G; CG-codex.md § G1 and § Selection; IDEAS.md G seeds 1 and 3.
- (a) Endpoint aliases or renames: a Rust item's name is not a provider fact (C1). The observed renames exist only because a wrapper exists.
- (b) Tree-shaped generated names (`s3_api::bucket::GetObject`): this renames every generated type in 55 SDKs, the controls included (C3), to save ≈20 lines of rename re-exports.
- (c) Re-export conventions: ≈2 lines per SDK, and Rust packaging, not a provider fact (both lanes).

Recorded limit: friction G1 (13 SDKs cannot find Concord types such as `ErrorKind` or pagination types where they look) is a Concord crate-layout fix, not a declaration statement.

## G8. Publish the generated tree and delete pure synonyms — MERGED into G1
Sources: CG-codex.md § G1.
Design: no language change. Once C1 and C2 make the generated client safe to hand out, delegates whose body is only navigation go: Datadog `client.rs:121-146` (23 non-blank lines, checked) and OpenAI `create_batch` (3). These are the forwarders G1 enables removing. Delegates that carry behavior stay (OpenAI `create_response` sets `stream: false`, Elasticsearch `get_document` checks the returned id).
