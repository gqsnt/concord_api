# Concord v5 — Complete Master Implementation Plan

Audience: Codex implementation agent  
Role of this document: single source of truth for implementing Concord v5  
Mode: contract-first, TDD-first, milestone-based  
Compatibility target: v5 only. v4 and pre-v4 syntax are migration diagnostics only.  
Priority order: DSL → generated client usage → core/runtime → macros  
Status: expanded master plan replacing the previous lightweight `all_v5.md`

---

## 0. How to use this document

This document is intentionally detailed.

It is not a high-level roadmap. It is an implementation contract.

The agent must follow this order:

```text
1. Lock the v5 DSL contract.
2. Lock the generated client usage contract.
3. Align the core only where those contracts require it.
4. Implement the macros as a compiler from DSL to usage + core plans.
5. Prove everything with fixtures, snapshots, runtime tests, docs, and audits.
```

Do not implement v5 by randomly changing macro codegen first.

Do not rewrite the core.

Do not add new product concepts unless this document explicitly requires them.

Do not claim completion unless the final report has no `no`, no `fail`, and no dirty audit results.

---

## 1. Master vision

Concord v5 is not a feature expansion.

Concord v5 is a clarity and strictness release.

The product must feel like this:

```text
api! DSL
    = structured upstream API documentation

generated client
    = typed Rust navigation through that documentation

core/runtime
    = stable plan executor, mostly invisible to normal users

macros
    = compiler from DSL to generated client + core plans
```

The v5 direction:

```text
client = root
scope = branch
endpoint = stanza/leaf
policy = inherited behavior
generated facade = primary API
explicit endpoint structs = advanced API
core = RequestPlan executor
macros = RawAst -> NormApiTree -> ResolvedApi -> codegen
```

---

## 2. Absolute priority order

The priority order is binding:

```text
1. DSL
2. Generated client usage
3. Core/runtime
4. Macros
```

This means:

- If macro implementation convenience makes the DSL uglier, the macro design is wrong.
- If the core leaks into normal generated usage, the public API boundary is wrong.
- If the DSL is readable but the generated client is awkward, the DSL/codegen contract is incomplete.
- If a core feature is elegant but not needed by DSL/usage, it does not belong in v5.
- If old syntax support makes the parser/codegen messy, keep it as diagnostics only.

---

## 3. Final v5 product example

### 3.1 Canonical DSL

```rust
api! {
    client RiotClient {
        base https "riotgames.com"

        secret api_key: String
        credential riot = api_key(secret.api_key)

        headers {
            "user-agent" = "RiotClient/1.0"
        }

        default {
            retry read
            rate_limit app
        }

        retry read {
            max_attempts 2
            methods [GET]
            on [429, 500, 502, 503, 504]
            retry_after
        }

        observe rate_limit RiotRateLimitHeaders

        rate_limit app {
            bucket application by [host] {
                500 / 10s
                30000 / 10m
            }
        }

        rate_limit match_v5_method {
            bucket method by [host, endpoint] {
                2000 / 10s
            }
        }
    }

    scope regional(region: RegionalRoute) {
        host [region, "api"]

        scope match_v5_matches {
            path ["lol", "match", "v5", "matches"]

            GET GetMatchIdsByPuuid(
                puuid: String,
                queue?: u16,
                start_time?: i64,
                end_time?: i64,
                start: u64 = 0,
                count: u64 = 20,
            )
                as ids_by_puuid
                path ["by-puuid", puuid, "ids"]
                query {
                    queue
                    "startTime" = start_time
                    "endTime" = end_time
                    start
                    count
                    "range" = fmt[start, "-", count]
                }
                paginate OffsetLimitPagination {
                    offset = start
                    limit = count
                }
                -> Json<Vec<String>>
                rate_limit match_v5_method

            GET GetMatch(match_id: String)
                as by_id
                path [match_id]
                -> Json<MatchDto>
                rate_limit match_v5_method

            GET GetTimeline(match_id: String)
                as timeline
                path [match_id, "timeline"]
                -> Json<TimelineDto>
                rate_limit match_v5_method
        }
    }
}
```

### 3.2 Canonical generated usage

```rust
let riot = RiotClient::new(api_key);

let ids = riot
    .regional(region)
    .match_v5_matches()
    .ids_by_puuid(puuid)
    .count(100)
    .paginate()
    .max_items(10_000)
    .collect()
    .await?;

let match_dto = riot
    .regional(region)
    .match_v5_matches()
    .by_id(match_id)
    .await?;
```

### 3.3 Canonical session-auth usage

```rust
api.auth_api()
    .login_for_session(login)
    .acquire_as_session()
    .await?;

let me = api.protected().me().await?;
```

### 3.4 Core execution shape

Generated endpoint code must end in:

```text
Endpoint::plan
    -> RequestPlan
    -> ApiClient::execute_plan
```

No v5 implementation should reintroduce:

```text
LegacyEndpoint
RoutePart
PolicyPart
AuthPart
BodyPart
PaginationPart
```

### 3.5 Macro compilation shape

```text
tokens
  -> RawAst
  -> legacy diagnostics / v5 validation
  -> NormApiTree
  -> ResolvedApi / ResolvedEndpoint
  -> codegen
  -> generated facade + builders + Endpoint::plan + auth state + rustdoc
```

---

## 4. Non-goals for v5

Do not implement these in v5 initial release:

```text
string path syntax: path "/users/{id}"
fmt "hello {name}" format-string parser
auth any/all
custom auth placement
generic middleware DSL
cache backend/storage DSL
automatic alias inference
automatic login
automatic refresh unless already explicit via provider
automatic pagination
rate-limit syntax redesign
pagination syntax redesign
scope aliases as a required feature
generic profiles { ... } wrapper
```

These may be reconsidered only after v5 is stable.

---

## 5. Unified design laws

### Law 1 — DSL describes upstream API contract

Allowed in DSL:

```text
base URL
host/path structure
endpoint method and params
query/header contract
body
response
mapping
auth placement
cache semantics
retry semantics
rate-limit contract
pagination contract
```

Not allowed in DSL:

```text
transport implementation
cache backend
cache capacity
distributed limiter backend
metrics sink
runtime hooks
threading/deployment concerns
```

### Law 2 — Route structure before formatting

Correct:

```rust
path ["users", id, "posts"]
```

Incorrect:

```rust
path [fmt["users/", id, "/posts"]]
```

`fmt[...]` produces one atom.

### Law 3 — `fmt[...]` is structured formatting, not Rust `format!`

Canonical:

```rust
fmt["prefix", value, "suffix"]
```

Allowed contexts:

```text
one host label
one path segment
one query value
one header value
```

`fmt[...]` must not become a string-template mini-language in v5 initial release.

### Law 4 — Facade-first usage

Primary:

```rust
api.scope().endpoint(args).await?;
```

Advanced:

```rust
api.request(endpoints::scope::Endpoint::new(args))
    .execute()
    .await?;
```

### Law 5 — Required values are direct arguments

Correct:

```rust
api.users().get(id).await?;
```

Incorrect:

```rust
api.users().get().id(id).await?;
```

### Law 6 — Optional/default values are setters

```rust
api.items()
    .list()
    .count(100)
    .filter("ranked")
    .await?;
```

### Law 7 — Complex behavior is explicit

No hidden auto-pagination.

```rust
api.items().list().paginate().collect().await?;
```

No hidden auto-login.

```rust
api.auth_api().login_for_session(login).acquire_as_session().await?;
```

### Law 8 — Core executes resolved plans, not DSL syntax

Core must not know about:

```text
fmt[...]
part[...]
endpoint stanza
query shorthand
raw macro AST
legacy diagnostics
```

### Law 9 — Macros are a compiler pipeline

Parser may know syntax.  
Codegen must not.

Raw AST may be syntax-shaped.  
ResolvedEndpoint must be semantic.

### Law 10 — v5 reduces tolerated syntax

Old syntax is rejected with migration diagnostics. It is not supported behavior.

---

## 6. Synchronization matrix

This table is the core of the plan.

Every v5 change must line up across DSL, usage, core, macros, and tests.

| Concept | DSL contract | Generated usage | Core impact | Macro impact | Required tests |
| --- | --- | --- | --- | --- | --- |
| Endpoint stanza | `GET Name(args) ... -> Json<T>` | facade endpoint method | none | parse endpoint lines; validate response/map/paginate | pass/fail DSL fixtures; generated facade compile |
| Endpoint alias | `as ids_by_puuid` | `.ids_by_puuid(...)` | none | resolve alias; collision diagnostics | alias method; duplicate alias fail |
| Required params | `id: u64` | `.get(id)` | stored in endpoint plan args | direct method args; endpoint struct constructor | compile pass/fail |
| Optional params | `filter?: String` | `.filter(v)`, `.maybe_filter(opt)`, `.clear_filter()` | args optional in RequestArgs | generate builder setters | usage fixtures |
| Defaulted params | `count: u64 = 20` | `.count(100)` optional override | default value in args/plan | generate defaulted field and setter | default/no-setter tests |
| `fmt[...]` | `fmt["u-", id]` | invisible to user | none; resolved before core | parse/resolve to format model | host/path/query/header tests |
| Query shorthand | `query { count }` | ordinary request query | none | normalize to `"count" = count` | shorthand tests |
| Headers | explicit string keys | generated request plan | header validation/encoding | resolve values/fmt | header value tests |
| `max_attempts` | `max_attempts 2` | no direct usage except docs | retry field/semantics rename | parse/resolve retry profile | retry runtime tests |
| Single default block | one `default {}` per node | inherited policy behavior | none | validate duplicates | duplicate default fail |
| Auth credential | `credential session = endpoint ...` | `.acquire_as_session()` | auth-state acquire primitive | detect endpoint-backed credential | auth session e2e |
| Cache profile | semantic DSL | `.cache_bypass()` etc per request | cache runtime | resolve policy to plan | cache behavior tests |
| Rate-limit profile | bucket DSL | no normal direct knob | rate limiter runtime | resolve plan | rate-limit e2e |
| Pagination | `paginate OffsetLimit...` | `.paginate().collect()` | PaginatedRequest and PaginationPlan | generate PaginationPlan and method | pagination e2e |
| `configure` | not DSL | `.configure(|cfg| ...)` | RuntimeConfig complete | generate wrapper | config tests |
| Explicit endpoint API | not primary DSL | `api.request(Endpoint::new(...))` | Endpoint::plan | generate endpoint structs | explicit advanced tests |
| Rustdoc | docs from resolved endpoint | IDE hover | none | generate docs | rustdoc snapshots |
| Old syntax | rejected | not applicable | none | legacy diagnostics only | compile-fail UI |

---

## 7. Milestone overview

```text
M0  Contract and test harness lock
M1  DSL grammar fixtures
M2  Generated usage fixtures
M3  Core alignment and invariants
M4  Macro parser and RawAst
M5  Normalization and semantic resolution
M6  Codegen facade/builders/Endpoint::plan
M7  Runtime integration and e2e tests
M8  Docs, examples, migration guide
M9  Final audit and release gate
```

Dependency direction:

```text
M0 -> M1 -> M2 -> M3 -> M4 -> M5 -> M6 -> M7 -> M8 -> M9
```

In practice, some work can happen in parallel, but Codex should not merge a later milestone unless its dependencies are satisfied.

---

# Milestone 0 — Contract and Test Harness Lock

## Goal

Freeze v5 as a contract before implementation.

## PR 001 — Commit v5 master documents

### Files

```text
docs/v5/all_v5.md
docs/v5/dsl_v5.md
docs/v5/usage_v5.md
docs/v5/core_v5.md
docs/v5/macros_v5.md
docs/v5/README.md
```

### Tasks

1. Add all five v5 planning docs.
2. Add `docs/v5/README.md` with this order:
   ```text
   1. DSL
   2. Usage
   3. Core
   4. Macros
   5. Master plan
   ```
3. Add non-goals list.
4. Add “do not claim complete unless final report is clean”.

### Acceptance

- docs exist;
- all docs agree on:
  - endpoint stanza;
  - `fmt[...]`;
  - query shorthand;
  - `max_attempts`;
  - single default;
  - facade-first usage;
  - plan-based core;
  - RawAst -> NormApiTree -> ResolvedApi -> codegen.

---

## PR 002 — Add unified v5 test layout

### Files

```text
concord_macros/tests/v5/
concord_macros/tests/v5/dsl/pass/
concord_macros/tests/v5/dsl/fail/
concord_macros/tests/v5/usage/pass/
concord_macros/tests/v5/usage/fail/
concord_macros/tests/v5/snapshots/resolved/
concord_macros/tests/v5/snapshots/generated/

concord_core/tests/v5_core/
concord_examples/src/v5/
```

### Tasks

1. Add empty harness modules.
2. Add naming conventions:
   ```text
   pass_* = should compile
   fail_* = should fail with diagnostics
   snapshot_* = should emit stable debug/generated output
   ```
3. Add an initial ignored test list only if implementation is not ready.
4. Each ignored test must include:
   ```text
   TODO(v5): why ignored, which PR enables it
   ```

### Acceptance

- harness compiles;
- ignored tests are visible and justified;
- final state must have no ignored v5 tests unless explicitly advanced/future-only.

---

# Milestone 1 — DSL Grammar Fixtures

## Goal

Define and test the DSL before implementation details.

## PR 003 — Endpoint stanza DSL fixtures

### Pass fixtures

```rust
GET Me
    as me
    path ["me"]
    -> Json<User>
```

```rust
POST CreatePost(body: Json<NewPost>)
    as create
    path ["posts"]
    -> Json<Post>
```

```rust
GET Search(q: String, page?: u32)
    as search
    path ["search"]
    query {
        q
        page
    }
    -> Json<SearchResponse>
```

```rust
POST LoginForSession(body: Json<LoginRequest>)
    as login_for_session
    path ["login"]
    auth header "X-Upstream-Key" = upstream
    -> Json<LoginResponse>
    map AccessToken {
        AccessToken::new(r.access_token)
    }
```

### Fail fixtures

```rust
GET Broken
    path ["broken"]
```

Expected:

```text
endpoint `Broken` is missing a response line `-> ...`
```

```rust
GET Broken
    path ["broken"]
    -> Json<A>
    -> Json<B>
```

Expected:

```text
endpoint `Broken` defines response more than once
```

```rust
GET Broken
    map Out { r }
    -> Json<A>
```

Expected:

```text
`map` must appear after the response line
```

### Acceptance

- fixtures exist;
- expected diagnostics are documented;
- old outer endpoint block is not canonical.

---

## PR 004 — `fmt[...]` DSL fixtures

### Pass fixtures

```rust
host [fmt["tenant-", tenant_id], "api"]
path ["users", fmt["u-", user_id]]
query { "range" = fmt[start, "-", count] }
headers { "x-trace" = fmt["trace-", trace_id] }
```

### Optional omission fixture

```rust
GET Search(prefix?: String)
    path ["search"]
    query {
        "q" = fmt["prefix:", prefix]
    }
    -> Json<Vec<Item>>
```

Expected:

```text
prefix = None  => no q
prefix = Some  => q=prefix:<value>
```

### Fail fixtures

```rust
fmt[]
```

Expected:

```text
fmt[...] requires at least one piece
```

```rust
path [fmt["key-", secret.api_key]]
```

Expected:

```text
secrets are not allowed in host/path formatting
```

```rust
part["u-", id]
```

Expected:

```text
part[...] was renamed to fmt[...] in v5
```

### Acceptance

- `fmt[...]` is specified as one atom;
- `part[...]` is migration-only.

---

## PR 005 — Query shorthand DSL fixtures

### Pass fixtures

```rust
query {
    count
    page
    "startTime" = start_time
}
```

### Required cases

1. endpoint param shorthand;
2. optional param shorthand;
3. defaulted param shorthand;
4. scope param shorthand if allowed;
5. client var shorthand if allowed;
6. mixed shorthand/explicit query.

### Fail fixture

```rust
query {
    cout
}
```

Expected:

```text
unknown query value `cout`
did you mean `count`?
```

### Acceptance

- shorthand is canonical;
- explicit same-name assignment may be accepted but discouraged.

---

## PR 006 — Retry and default DSL fixtures

### Pass

```rust
retry read {
    max_attempts 2
    methods [GET]
    on [429, 500, 503]
    retry_after
}
```

```rust
default {
    retry read
    rate_limit app
}
```

### Fail

```rust
retry read {
    attempts 2
}
```

Expected:

```text
attempts was renamed to max_attempts
```

```rust
default { retry read }
default { rate_limit app }
```

Expected:

```text
multiple default blocks are not allowed in v5
```

### Acceptance

- `max_attempts` contract is explicit;
- single default rule is explicit.

---

## PR 007 — Old syntax fail fixtures

### Required fail syntax

```text
scheme:
host:
auth { credential ... }
use_auth HeaderAuth(...)
use_auth BearerAuth(...)
response custom
route.host
backoff none
limit 500 every 10 seconds
old mapping `| Out =>`
auth any/all
custom auth placement
old endpoint outer block
```

### Acceptance

- every old syntax has a compile-fail test;
- each diagnostic gives a v5 replacement or clear rejection reason.

---

# Milestone 2 — Generated Usage Fixtures

## Goal

Define generated Rust surface before codegen.

## PR 008 — Client construction and configure fixtures

### Pass usage

```rust
let api = Api::new(api_key);

let api = Api::builder()
    .api_key(api_key)
    .tenant(tenant)
    .build()?;

let api = Api::new(api_key)
    .configure(|cfg| {
        cfg.debug(DebugLevel::V);
        cfg.pagination(Caps::default().max_items(10_000));
    });
```

### Advanced config usage

```rust
let api = Api::new(api_key)
    .configure(|cfg| {
        cfg.cache_store(cache);
        cfg.rate_limiter(limiter);
        cfg.transport(transport);
    });
```

### Acceptance

- `new` exists;
- `builder` exists;
- missing builder required fields produce clear error;
- `configure` is the subsystem extension path.

---

## PR 009 — Facade navigation fixtures

### Pass usage

```rust
api.regional(region)
    .match_v5_matches()
    .ids_by_puuid(puuid)
    .await?;
```

### Required cases

1. root scope method;
2. nested scope method;
3. scope param direct argument;
4. endpoint alias method;
5. inherited scope param in route;
6. endpoint with no alias uses predictable snake_case.

### Fail cases

1. duplicate alias in same scope;
2. alias collision with child scope if applicable.

### Acceptance

- normal user never needs `api.request(...)` for normal examples.

---

## PR 010 — Param builder usage fixtures

### Required params

```rust
api.users().get(id).await?;
```

### Optional params

```rust
api.search()
    .q("rust")
    .maybe_page(page)
    .clear_q()
    .await?;
```

### Defaulted params

```rust
api.items()
    .list()
    .count(100)
    .await?;
```

### Acceptance

- required params are direct args;
- optional/default params are setters;
- no common example uses `Some(...)`.

---

## PR 011 — Execution, decoded response, pagination, auth usage fixtures

### Direct execution

```rust
api.users().get(42).await?;
api.users().get(42).execute().await?;
```

### Decoded response

```rust
let res = api.users().get(42).execute_decoded().await?;
res.status();
res.headers();
res.url();
res.value();
res.into_value();
```

### Pagination

```rust
api.items()
    .list()
    .count(100)
    .paginate()
    .max_items(1_000)
    .collect()
    .await?;
```

### Auth session

```rust
api.auth_api()
    .login_for_session(login)
    .acquire_as_session()
    .await?;

api.protected().me().await?;
```

### Explicit advanced endpoint

```rust
let ep = endpoints::users::GetUser::new(42);

api.request(ep)
    .execute()
    .await?;
```

### Acceptance

- all usage fixtures define exact public generated API.

---

# Milestone 3 — Core Alignment and Invariants

## Goal

Align the core with v5 usage while preserving the plan-based architecture.

The core should not know DSL syntax.

## PR 012 — Core retry `max_attempts`

### Files likely touched

```text
concord_core/src/retry/*
concord_core/src/runtime/*
concord_core/src/policy/*
concord_macros/src/sema/retry.rs
concord_macros/src/codegen/*
```

### Tasks

1. Rename retry fields from `attempts` to `max_attempts`.
2. Define semantics:
   ```text
   max_attempts = total attempts including first request
   ```
3. Validate:
   ```text
   max_attempts >= 1
   ```
4. Update retry loop.
5. Update debug/error messages.
6. Update tests.

### Tests

1. `max_attempts = 1` sends once.
2. `max_attempts = 2` sends at most twice.
3. `max_attempts = 0` fails validation.
4. retry-after is honored.
5. no double sleep with rate-limit cooldown.
6. no use of old `attempts` in core source.

### Acceptance

- core API and macros use `max_attempts`;
- old `attempts` remains only migration diagnostic.

---

## PR 013 — RuntimeConfig completeness

### Files likely touched

```text
concord_core/src/runtime/config.rs
concord_core/src/client.rs
concord_core/src/prelude.rs
concord_core/src/advanced.rs
concord_macros/src/codegen/client.rs
```

### Tasks

1. Ensure `RuntimeConfig` supports:
   - debug;
   - debug sink if present;
   - runtime hooks;
   - cache store;
   - inflight policy/registry;
   - rate limiter;
   - retry policy;
   - max auth retries;
   - pagination caps;
   - transport if architecture uses it there.
2. Ensure generated clients can expose:
   ```rust
   .configure(|cfg| { ... })
   ```
3. Keep direct subsystem setters out of normal docs.

### Tests

1. configure debug.
2. configure cache store.
3. configure rate limiter.
4. configure transport.
5. configure pagination caps.

### Acceptance

- usage fixtures around `configure` pass.

---

## PR 014 — DecodedResponse user surface and error context

### Files likely touched

```text
concord_core/src/response/*
concord_core/src/error.rs
concord_core/src/runtime/*
```

### Tasks

1. Ensure `DecodedResponse<T>` exposes:
   - `status()`;
   - `headers()`;
   - `url()`;
   - `value()`;
   - `into_value()`;
   - user-facing metadata.
2. Ensure no internal types are required.
3. Improve error context for:
   - missing credential;
   - decode;
   - pagination loop;
   - rate-limit;
   - cache;
   - transport.
4. Add redaction for secrets.

### Tests

1. `execute_decoded()` usage fixture.
2. decode error includes endpoint/status/content-type.
3. missing credential is actionable.
4. redaction tests.

### Acceptance

- `execute_decoded` is useful without internal leakage.

---

## PR 015 — Runtime invariant tests

### Files

```text
concord_core/tests/v5_core/runtime_order.rs
concord_core/tests/v5_core/auth.rs
concord_core/tests/v5_core/cache.rs
concord_core/tests/v5_core/rate_limit.rs
concord_core/tests/v5_core/pagination.rs
concord_core/tests/v5_core/inflight.rs
```

### Required invariants

1. Auth before cache.
2. Fresh cache hit skips transport.
3. Fresh cache hit skips rate-limit.
4. Stale revalidation goes through limiter and transport.
5. Rate-limit acquire before send.
6. Rate-limit observe after response.
7. Auth rejection before cache storage.
8. Retry decision before decode.
9. Retry on page N does not advance page.
10. Inflight dedupe does not merge different auth identities.
11. Cache key partitions by auth identity.

### Acceptance

- runtime behavior is locked before macro integration.

---

## PR 016 — Public API boundary

### Files likely touched

```text
concord_core/src/prelude.rs
concord_core/src/advanced.rs
concord_core/src/internal.rs
docs/*
concord_examples/*
```

### Tasks

1. Keep normal API in `prelude`.
2. Keep extension API in `advanced`.
3. Keep generated plumbing in `internal`.
4. Add export snapshots.
5. Ensure normal examples do not import `internal`.

### Audit

```bash
rg "use concord_core::internal" concord_examples docs
rg "RequestPlan|EndpointPlan|AuthPlan|CredentialSlot|RateLimitPermit|CacheKey|runtime_state" concord_examples docs
```

### Acceptance

- public API boundary is clean.

---

# Milestone 4 — Macro Parser and RawAst

## Goal

Parse v5 syntax into RawAst with spans and diagnostics.

## PR 017 — Macro v5 parser foundation

### Files likely touched

```text
concord_macros/src/parse/mod.rs
concord_macros/src/parse/client.rs
concord_macros/src/parse/scope.rs
concord_macros/src/parse/endpoint.rs
concord_macros/src/parse/policy.rs
concord_macros/src/parse/value.rs
concord_macros/src/ast/raw.rs
```

### Tasks

1. Define `RawApi`.
2. Define `RawClient`.
3. Define `RawScope`.
4. Define `RawEndpoint`.
5. Define `RawEndpointLine`.
6. Preserve spans.
7. Parse endpoint stanzas.
8. Parse nested blocks:
   - query;
   - headers;
   - paginate;
   - map.
9. Parse `fmt[...]`.
10. Parse `max_attempts`.

### Acceptance

- DSL pass fixtures parse;
- fail fixtures have structured errors.

---

## PR 018 — Legacy diagnostics isolation

### Files likely touched

```text
concord_macros/src/parse/legacy.rs
concord_macros/src/sema/diagnostics.rs
concord_macros/tests/v5/dsl/fail/*
```

### Tasks

1. Detect old syntax.
2. Emit v5 migration diagnostics.
3. Do not normalize old syntax.
4. Do not let old syntax reach codegen.

### Required diagnostics

```text
scheme:/host:      -> base https "example.com"
use_auth HeaderAuth -> auth header "X-Api-Key" = key
auth { credential } -> credential in client body
part[...]           -> fmt[...]
attempts            -> max_attempts
response custom     -> observe rate_limit
route.host          -> host
auth any/all        -> unsupported
custom auth         -> unsupported; use provider + supported placement
old endpoint block  -> endpoint stanza
```

### Acceptance

- all old syntax fail fixtures pass;
- diagnostics are actionable.

---

# Milestone 5 — Normalization and Semantic Resolution

## Goal

Convert RawAst into clean semantic models.

## PR 019 — NormApiTree

### Files likely touched

```text
concord_macros/src/model/norm.rs
concord_macros/src/sema/normalize.rs
```

### Tasks

1. Define `NormApiTree`.
2. Define `NormNode`.
3. Define `NormEndpoint`.
4. Normalize endpoint stanza lines.
5. Normalize query shorthand.
6. Normalize `fmt[...]`.
7. Normalize `max_attempts`.
8. Reject duplicate defaults.

### Acceptance

- NormApiTree snapshots exist;
- no legacy syntax in normalized tree.

---

## PR 020 — ResolvedApi / ResolvedEndpoint model

### Files likely touched

```text
concord_macros/src/model/resolved.rs
concord_macros/src/sema/resolve.rs
```

### Tasks

1. Define `ResolvedApi`.
2. Define `ResolvedClient`.
3. Define `ResolvedScope`.
4. Define `ResolvedEndpoint`.
5. Include:
   - endpoint type;
   - facade method;
   - module path;
   - facade path;
   - method;
   - params;
   - route;
   - policy;
   - body;
   - response;
   - mapping;
   - pagination;
   - docs;
   - span.
6. Ensure no raw AST ancestry is required.

### Acceptance

- resolved model snapshots can be generated.

---

## PR 021 — Route, params, values, and inheritance resolver

### Tasks

1. Resolve client vars/secrets.
2. Resolve scope params.
3. Resolve endpoint params.
4. Resolve host/path inheritance.
5. Resolve route pieces.
6. Resolve `fmt[...]` into format model.
7. Reject secrets in host/path formatting.
8. Resolve query shorthand.
9. Resolve headers/query values.
10. Suggest unknown names.

### Acceptance

- route/value snapshots pass;
- unknown-name diagnostics pass.

---

## PR 022 — Policy/profile resolver

### Tasks

1. Resolve auth credentials.
2. Resolve endpoint-backed credentials.
3. Resolve retry profiles with `max_attempts`.
4. Resolve cache profiles.
5. Resolve rate-limit profiles.
6. Resolve pagination fields.
7. Resolve response/mapping output type.
8. Resolve policy inheritance once:
   ```text
   client -> scope -> endpoint
   ```

### Acceptance

- inherited policy snapshots pass;
- unknown credential/profile diagnostics pass;
- codegen has all required semantic data.

---

# Milestone 6 — Codegen

## Goal

Generate v5 client usage and core plans from resolved data only.

## PR 023 — Codegen boundary enforcement

### Files

```text
concord_macros/src/codegen/*
```

### Tasks

1. Codegen takes `ResolvedApi`.
2. Remove raw AST imports from codegen.
3. Move shared non-parser enums to `model/common.rs` if needed.
4. Add grep/check test for forbidden imports.

### Forbidden in codegen

```text
ClientDef
LayerDef
EndpointDef
AuthBlock
RetryProfilesBlock
CacheProfilesBlock
RateLimitProfilesBlock
LegacySyntax
```

### Acceptance

- codegen is resolved-only.

---

## PR 024 — Generate client constructors, builder, configure

### Tasks

1. Generate `Api::new(...)`.
2. Generate `Api::builder()`.
3. Generate builder setters.
4. Generate missing required field errors.
5. Generate `configure`.
6. Generate direct helpers:
   - `with_debug_level`;
   - `with_pagination_caps`;
   - `new_with_transport` if supported.

### Acceptance

- construction/config usage fixtures pass.

---

## PR 025 — Generate facade scopes and endpoint methods

### Tasks

1. Generate root scope methods.
2. Generate nested scope handles.
3. Store scope params.
4. Preserve parent params.
5. Generate endpoint facade methods.
6. Required endpoint params are direct args.
7. Alias becomes method name.
8. Derived snake_case fallback works.

### Acceptance

- facade usage fixtures pass.

---

## PR 026 — Generate request builders and explicit endpoint API

### Tasks

1. Generate optional setters:
   - `.field(value)`;
   - `.maybe_field(option)`;
   - `.clear_field()`.
2. Generate defaulted setters.
3. Generate endpoint structs.
4. Generate `Endpoint::new(required...)`.
5. Ensure explicit endpoint path and facade path produce same `RequestPlan`.

### Acceptance

- builder usage fixtures pass;
- explicit advanced path fixtures pass.

---

## PR 027 — Generate Endpoint::plan

### Tasks

1. Generate `impl Endpoint<Cx>`.
2. Build `ResolvedRoute`.
3. Build `ResolvedPolicy`.
4. Build `BodyPlan`.
5. Build `ResponsePlan`.
6. Build `PaginationPlan`.
7. Return `RequestPlan`.

### Forbidden

```text
LegacyEndpoint
RoutePart impl
PolicyPart impl
AuthPart impl
BodyPart impl
PaginationPart impl
```

### Acceptance

- generated plan snapshots pass.

---

## PR 028 — Generate auth state and `acquire_as_*`

### Tasks

1. Detect endpoint-backed credentials.
2. Generate auth-state handles.
3. Generate `.acquire_as_<credential>()`.
4. Ensure method absent for non-credential endpoints.
5. Generate rustdoc.

### Acceptance

- auth usage fixtures pass.

---

## PR 029 — Generate pagination usage

### Tasks

1. Generate `.paginate()` for paginated endpoints.
2. Ensure `.max_items`, `.max_pages`, `.collect` work.
3. Preserve optional/default params through pagination.
4. Do not expose `.paginate()` for non-paginated endpoints if feasible.

### Acceptance

- pagination usage fixtures pass.

---

## PR 030 — Generate rustdoc

### Tasks

Generate rustdoc for:

```text
client
builder
scope methods
endpoint facade methods
endpoint structs
request setters
auth acquire methods
pagination methods
```

Endpoint rustdoc includes:

```text
HTTP method
path
required params
query params
auth summary
cache/retry/rate-limit profile names
pagination summary
response/output type
```

Must not leak secrets.

### Acceptance

- rustdoc snapshots pass;
- `cargo doc` passes.

---

# Milestone 7 — Runtime Integration and End-to-End Tests

## Goal

Prove DSL + usage + core + macros work together.

## PR 031 — Minimal and facade E2E tests

### Tests

1. minimal client request.
2. nested scope request.
3. alias endpoint request.
4. explicit endpoint request.
5. facade and explicit endpoint produce same plan.

### Acceptance

- generated code executes through `execute_plan`.

---

## PR 032 — Auth session E2E tests

### Tests

1. login endpoint acquires session.
2. protected endpoint succeeds after acquire.
3. protected endpoint before acquire errors actionably.
4. clear session works.
5. acquire method absent for non-credential endpoint.

### Acceptance

- session auth is explicit and reliable.

---

## PR 033 — Pagination E2E tests

### Tests

1. offset-limit pagination.
2. cursor pagination.
3. paged pagination.
4. max_items.
5. max_pages.
6. retry on page N.
7. rate-limit on page N.
8. loop detection.

### Acceptance

- pagination usage is production-safe.

---

## PR 034 — Cache/retry/rate-limit/inflight E2E tests

### Tests

1. cache hit skips transport.
2. cache hit skips limiter.
3. cache refresh works.
4. cache bypass works.
5. retry `max_attempts` works.
6. retry-after no double sleep.
7. rate-limit observer works.
8. inflight dedupe respects auth identity.
9. protected cache responses do not leak across credentials.

### Acceptance

- major runtime interactions are proven.

---

# Milestone 8 — Docs, Examples, Migration

## Goal

Update user-facing materials to match final v5.

## PR 035 — Canonical v5 examples

### Files

```text
concord_examples/src/v5/minimal.rs
concord_examples/src/v5/auth_session.rs
concord_examples/src/v5/pagination.rs
concord_examples/src/v5/riot_like.rs
concord_examples/src/v5/runtime_config.rs
concord_examples/src/v5/explicit_endpoint.rs
concord_examples/src/v5/fmt.rs
```

### Requirements

1. Examples compile.
2. Examples use endpoint stanzas.
3. Examples use `fmt[...]`.
4. Examples use query shorthand.
5. Examples use `max_attempts`.
6. Examples use one default block.
7. Examples are facade-first.
8. Explicit endpoint API appears only in advanced example.

---

## PR 036 — Migration guide v4 → v5

### File

```text
docs/v5/migration_v4_to_v5.md
```

### Required sections

1. Endpoint block → endpoint stanza.
2. `part[...]` → `fmt[...]`.
3. `attempts` → `max_attempts`.
4. Multiple defaults → single default.
5. `query { x = x }` → `query { x }` style.
6. `use_auth` → `auth header/bearer`.
7. `response custom` → `observe rate_limit`.
8. `route.host` → `host`.
9. `auth any/all` unsupported.
10. Custom auth placement unsupported.
11. Explicit endpoint API remains advanced.

### Acceptance

- migration guide diagnostics match compiler messages.

---

## PR 037 — Public docs update

### Files

```text
docs/00-quick-start.md
docs/01-mental-model.md
docs/02-dsl-overview.md
docs/03-generated-usage.md
docs/04-runtime-config.md
docs/05-auth.md
docs/06-pagination.md
docs/07-cache-retry-rate-limit.md
docs/16-dsl-reference.md
```

### Requirements

1. normal docs import `prelude`;
2. advanced docs import `advanced`;
3. normal docs never import `internal`;
4. normal docs show `configure`;
5. normal docs show facade-first usage;
6. generated rustdoc is mentioned as DX surface.

---

# Milestone 9 — Final Audit and Release Gate

## Goal

Prove v5 is complete.

## PR 038 — Unified audit script

### File

```text
scripts/audit_v5.sh
```

### Script

```bash
#!/usr/bin/env bash
set -euo pipefail

cargo fmt --check
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo doc --no-deps --all-features

# Old DSL syntax should not appear in normal examples/docs.
rg "scheme:|host:|use_auth|backoff none|response custom|route\\.host" concord_examples docs && exit 1 || true

# Old formatting/retry syntax should not appear outside migration diagnostics/tests.
rg "part\\[|\\battempts\\b" concord_examples docs concord_core/src concord_macros/src && exit 1 || true

# Legacy execution model must be gone.
rg "LegacyEndpoint|RoutePart|PolicyPart|AuthPart|BodyPart|PaginationPart" concord_core concord_macros concord_examples && exit 1 || true

# Codegen must not consume raw AST.
rg "ClientDef|LayerDef|EndpointDef|AuthBlock|RetryProfilesBlock|CacheProfilesBlock|RateLimitProfilesBlock|LegacySyntax" concord_macros/src/codegen && exit 1 || true

# Internal leakage into normal docs/examples must be absent.
rg "use concord_core::internal" concord_examples docs && exit 1 || true
rg "RequestPlan|EndpointPlan|AuthPlan|CredentialSlot|RateLimitPermit|CacheKey|runtime_state" concord_examples docs && exit 1 || true

# Normal docs should be facade-first.
rg "api\\.request\\(" concord_examples docs || true

# Auth session canonical usage should exist.
rg "acquire_as_" concord_examples docs
```

### Manual exceptions

Allowed only in:

```text
migration docs
compile-fail tests
advanced/internal docs
legacy diagnostic parser module
```

---

## PR 039 — Final status report

Codex agent must report exactly:

```text
Concord v5 status: complete / not complete

DSL:
- endpoint stanza canonical: yes/no
- fmt[...] canonical: yes/no
- part[...] rejected: yes/no
- query shorthand canonical: yes/no
- max_attempts implemented: yes/no
- attempts rejected: yes/no
- single default enforced: yes/no
- old syntax diagnostics complete: yes/no

Usage:
- facade-first usage works: yes/no
- required params direct args: yes/no
- optional/default setters work: yes/no
- maybe_* and clear_* work: yes/no
- direct await works: yes/no
- execute works: yes/no
- execute_decoded works: yes/no
- pagination fluent usage works: yes/no
- acquire_as_session works: yes/no
- explicit endpoint advanced path works: yes/no
- configure is canonical runtime extension path: yes/no
- generated rustdoc exists: yes/no

Core:
- Endpoint::plan primary model: yes/no
- RequestPlan primary execution input: yes/no
- EndpointPlan shape clean: yes/no
- fmt/query/stanza absent from core: yes/no
- max_attempts semantics tested: yes/no
- RuntimeConfig complete: yes/no
- runtime pipeline order tested: yes/no
- auth/cache/retry/rate-limit/pagination interactions tested: yes/no
- errors actionable and redacted: yes/no
- prelude/advanced/internal boundary clean: yes/no

Macros:
- RawAst -> NormApiTree -> ResolvedApi pipeline implemented: yes/no
- codegen consumes resolved data only: yes/no
- endpoint stanzas parsed: yes/no
- fmt[...] parsed/resolved: yes/no
- query shorthand resolved: yes/no
- max_attempts parsed/resolved: yes/no
- single default validated: yes/no
- legacy diagnostics isolated: yes/no
- facade/builders/auth/pagination/rustdoc generated: yes/no
- resolved snapshots added: yes/no
- generated code snapshots added: yes/no

Commands:
- cargo fmt --check: pass/fail
- cargo test --all-features: pass/fail
- cargo clippy --all-targets --all-features -- -D warnings: pass/fail
- cargo doc --no-deps --all-features: pass/fail

Grep audit:
- old DSL syntax clean: yes/no
- part/attempts clean: yes/no
- legacy runtime symbols clean: yes/no
- raw AST in codegen clean: yes/no
- internal leakage clean: yes/no

Remaining blockers:
- none / list
```

Rule:

```text
Do not report complete if any required item is no, fail, or not clean.
```

---

## 10. Expanded PR dependency table

| PR | Depends on | Unlocks |
| --- | --- | --- |
| PR001 docs | none | all v5 work |
| PR002 harness | PR001 | fixtures |
| PR003 endpoint DSL | PR002 | parser endpoint work |
| PR004 fmt DSL | PR002 | value parser/resolver |
| PR005 query DSL | PR002 | query normalization |
| PR006 retry/default DSL | PR002 | core retry + parser retry |
| PR007 old syntax | PR002 | legacy diagnostics |
| PR008 construction usage | PR002 | codegen client |
| PR009 facade usage | PR003 | codegen facade |
| PR010 params usage | PR003 | request builders |
| PR011 execution usage | PR008-PR010 | PendingRequest/codegen/runtime |
| PR012 core retry | PR006 | macro max_attempts/codegen |
| PR013 RuntimeConfig | PR008 | configure codegen |
| PR014 DecodedResponse/errors | PR011 | execute_decoded usage |
| PR015 runtime invariants | PR012-PR014 | e2e safety |
| PR016 API boundary | PR013-PR014 | docs/examples clean |
| PR017 parser | PR003-PR007 | normalization |
| PR018 legacy diagnostics | PR007, PR017 | old syntax tests |
| PR019 NormApiTree | PR017-PR018 | resolver |
| PR020 ResolvedApi | PR019 | codegen |
| PR021 route/value resolver | PR020 | endpoint plan generation |
| PR022 policy resolver | PR020 | generated policy plans |
| PR023 codegen boundary | PR020 | all codegen |
| PR024 client codegen | PR008, PR013, PR023 | construction tests |
| PR025 facade codegen | PR009, PR023 | facade tests |
| PR026 builder codegen | PR010, PR023 | param tests |
| PR027 Endpoint::plan | PR012, PR021, PR022, PR023 | runtime integration |
| PR028 auth acquire | PR011, PR022, PR024-PR027 | auth e2e |
| PR029 pagination | PR011, PR027 | pagination e2e |
| PR030 rustdoc | PR024-PR029 | docs/autocomplete |
| PR031 minimal e2e | PR024-PR027 | integration confidence |
| PR032 auth e2e | PR028 | auth complete |
| PR033 pagination e2e | PR029 | pagination complete |
| PR034 policy e2e | PR027, PR015 | runtime complete |
| PR035 examples | PR031-PR034 | docs |
| PR036 migration | PR018 | release docs |
| PR037 public docs | PR016, PR030, PR035 | release docs |
| PR038 audit script | PR035-PR037 | release gate |
| PR039 final report | PR038 | v5 completion |

---

## 11. Traceability checklist

For every v5 feature, the implementing agent must verify all four layers.

### Endpoint stanza

```text
DSL fixture: yes
Usage fixture: facade method works
Core effect: none
Macro: parser + validation + ResolvedEndpoint
```

### `fmt[...]`

```text
DSL fixture: host/path/query/header
Usage fixture: generated call produces correct URL/header
Core effect: none
Macro: parser + FmtSpec resolver
```

### Query shorthand

```text
DSL fixture: query { count }
Usage fixture: generated query correct
Core effect: none
Macro: normalize to explicit query entry
```

### `max_attempts`

```text
DSL fixture: max_attempts accepted, attempts rejected
Usage fixture: no direct usage, docs updated
Core effect: retry semantics rename
Macro: retry parser/resolver/codegen
```

### Single default

```text
DSL fixture: duplicate rejected
Usage fixture: inherited policy behavior still works
Core effect: none
Macro: normalization/validation
```

### `configure`

```text
DSL fixture: none
Usage fixture: configure compiles
Core effect: RuntimeConfig complete
Macro: generated wrapper
```

### `acquire_as_session`

```text
DSL fixture: endpoint-backed credential
Usage fixture: acquire_as_session compiles/runs
Core effect: auth-state acquire primitive
Macro: detect credential endpoint, generate method
```

### Pagination

```text
DSL fixture: paginate block
Usage fixture: .paginate().max_items().collect()
Core effect: PaginationPlan/PaginatedRequest stable
Macro: generate PaginationPlan and method
```

### Explicit endpoint API

```text
DSL fixture: endpoint exists
Usage fixture: endpoints::...::Endpoint::new(...)
Core effect: Endpoint::plan
Macro: generate endpoint struct
```

### Rustdoc

```text
DSL fixture: enough endpoint metadata
Usage fixture: IDE docs expected
Core effect: none
Macro: generate docs from ResolvedEndpoint
```

---

## 12. Risk register

### Risk 1 — The plan becomes too macro-driven

Symptom:

```text
codegen changes before DSL/usage fixtures exist
```

Mitigation:

```text
reject PRs that do not reference fixture contracts
```

### Risk 2 — Endpoint stanza parser is ambiguous

Symptom:

```text
unknown endpoint lines parsed as Rust exprs
```

Mitigation:

```text
strict endpoint-line keyword set
unknown line fail diagnostic
termination at HTTP method/scope/client/brace
```

### Risk 3 — `fmt[...]` becomes a path mini-language

Symptom:

```rust
path [fmt["users/", id, "/posts"]]
```

Mitigation:

```text
docs say one atom
optional diagnostic on slash in path fmt
tests for path segment behavior
```

### Risk 4 — Core rewrite spiral

Symptom:

```text
major changes to EndpointPlan/RequestPlan unrelated to tests
```

Mitigation:

```text
core changes limited to max_attempts, RuntimeConfig, DecodedResponse, errors, invariants
```

### Risk 5 — Usage surface becomes too large

Symptom:

```text
normal docs show many with_* runtime subsystem setters
```

Mitigation:

```text
configure is canonical
subsystem direct setters advanced only
grep audit
```

### Risk 6 — Raw AST leaks into codegen

Symptom:

```text
codegen imports ClientDef/LayerDef/EndpointDef
```

Mitigation:

```text
mandatory grep audit
ResolvedApi snapshots
```

### Risk 7 — Tests pass but docs teach old style

Symptom:

```text
docs show part[...], attempts, endpoint block, api.request primary
```

Mitigation:

```text
docs grep audit
migration-only exception
```

---

## 13. Final v5 completion definition

Concord v5 is complete only when all of these are true:

### DSL

1. Endpoint stanza is canonical.
2. `fmt[...]` is canonical.
3. `part[...]` is rejected with migration diagnostic.
4. Query shorthand is canonical.
5. Retry uses `max_attempts`.
6. `attempts` is rejected with migration diagnostic.
7. One default block per client/scope is enforced.
8. Old endpoint outer block is rejected with migration diagnostic.
9. Old pre-v4/v4 syntax is rejected with useful diagnostics.

### Usage

10. Facade-first usage works.
11. Required params are direct args.
12. Optional/default params are setters.
13. `maybe_*` and `clear_*` exist where practical.
14. Direct `.await` works.
15. `.execute()` works.
16. `.execute_decoded()` works.
17. Pagination is explicit and fluent.
18. `acquire_as_session()` works.
19. Explicit endpoint API remains stable advanced path.
20. `configure(...)` is the canonical runtime extension path.
21. Generated rustdoc is useful.

### Core

22. Core remains plan-based.
23. Core does not know DSL syntax.
24. Retry semantics use `max_attempts`.
25. RuntimeConfig is complete.
26. Runtime pipeline order is tested.
27. Auth/cache/retry/rate-limit/pagination interactions are tested.
28. Errors are actionable.
29. Secrets are redacted.
30. Public API boundary is clean.

### Macros

31. Parser accepts v5 syntax.
32. Parser rejects old syntax with migration diagnostics.
33. RawAst is parser-only.
34. NormApiTree contains no legacy behavior.
35. ResolvedApi/ResolvedEndpoint are codegen source of truth.
36. Codegen consumes resolved data only.
37. Generated endpoints implement `Endpoint::plan`.
38. Generated facade/builders/auth/pagination/rustdoc match usage contract.
39. Resolved snapshots exist.
40. Generated snapshots exist.

### Release

41. Examples compile.
42. Docs match v5 canonical style.
43. Migration guide matches diagnostics.
44. `cargo fmt --check` passes.
45. `cargo test --all-features` passes.
46. `cargo clippy --all-targets --all-features -- -D warnings` passes.
47. `cargo doc --no-deps --all-features` passes.
48. Unified grep audit is clean.
49. Final report has no `no`, no `fail`, no blockers.

---

## 14. Final instruction to Codex

Implement v5 as a contract-first compiler migration.

The safe order is:

```text
1. Write fixtures.
2. Lock generated usage expectations.
3. Align core invariants narrowly.
4. Parse v5.
5. Normalize v5.
6. Resolve v5.
7. Generate facade/builders/plans.
8. Prove runtime integration.
9. Update docs/examples.
10. Run final audit.
```

If a change makes the DSL less readable, stop.

If a change makes normal usage expose internals, stop.

If a change makes the core know DSL syntax, stop.

If codegen needs raw AST to work, stop.

Only declare v5 complete after the final report is clean.
