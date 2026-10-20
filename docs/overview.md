# Concord v3 overview

## Product thesis

Concord is a Rust library and `api!` declaration language for **typed, managed
HTTP clients**. Authors describe one client-shaped domain tree. Consumers hold
a client or scope, construct an inert typed request, select an appropriate
terminal and receive a typed result. The tree aligns value ownership,
navigation and lexical inheritance, but each configuration family has a law
appropriate to its data.

Concord is not an arbitrary HTTP middleware engine, a dynamic scripting
language, a generic serialization of provider schemas or a sandbox for hostile
Rust extensions. It does not infer business success, rollback or retry safety
from the absence of a usable response.

```text
author declaration
  -> resolved semantic model (one closed invocation)
  -> generated caller API with real rustdoc
  -> static effective contract, bound plans, concord.contract/3 export/diff
  -> one managed executor with truthful outcomes
```

The macro and core exist to make these projections truthful. They are not the
product center: the ordinary authoring and calling path is short, and nothing
consequential is implicit.

## The vocabulary

| Concept | Meaning |
|---|---|
| Owner | A client, scope or endpoint with its own declared values and attachments. |
| Snapshot | The selected, immutable ordinary values of one owner, including retained initializers. |
| Endpoint intent | Typed endpoint-local data and selected scope snapshots, not yet attached to an executable client. |
| Bound request | An endpoint intent combined with a valid client/owner execution binding. Still inert. |
| Operation | One polled execution, collection, traversal or credential-storage execution, with identity and finite work budgets. |
| Attempt | One application-request submission opportunity within a direct request or page. |
| Effective contract | Resolved declarations, bindings, capabilities and provenance, before runtime values. |
| Plan | A bounded public projection of one bound request, with secrets excluded and sensitive values redacted. |
| Outcome | Typed result plus truthful execution and completion information. |

Static, unbound, directly generated and runtime-bound request forms all enter
the same managed executor and the same public-head composition. The client
root is an owner, not a special empty prefix. See the
[mental model](mental_model.md).

## The language at a glance

This contract is maintained and compiled with the quick-start program:

<!-- concord-source: crates/concord-examples/src/getting_started/domain_tree.rs#domain_tree_contract -->
<!-- concord-dsl -->
```rust
api! {
    client CatalogApi {
        base "http://catalog.example.test/";
        timeout 30s;
        response Json;

        values {
            query tenant: String;
        }

        path "api";

        scope regional(region: String) {
            subdomain replace [region, "api"];
            path "v1/catalog";

            GET item_summaries(
                query category?: String,
                query offset: u64 = 0,
                query count: NonZeroU32 = DEFAULT_PAGE_SIZE,
            ) -> Vec<ItemSummary> {
                path "items";
                paginate via OffsetPagination {
                    init OffsetInit { offset: offset, page_size: count };
                    cursor -> offset;
                }
            }
        }
    }
}
```

- The root `values` block owns client values; scope and endpoint parameter
  lists own theirs. `query tenant: String` is a direct placement: the value is
  introduced and emitted as the `tenant` query key.
- `?` selects outer presence (`category` is omitted when absent). `= 0` retains
  an initializer; `#[default]` retains the payload's `Default`.
- `path` clauses append relative templates; `subdomain` lists prepend typed DNS
  labels. `replace` and `off` affect generated segments/labels only; the base
  stays fixed.
- `response Json` is an explicit representation choice; nothing is guessed from
  `Vec<ItemSummary>`.
- `paginate via` makes `pages`, `collect_prefix` and `collect_all` available.
  Ordinary `.await` still returns one page.

The canonical extended fixture, with credentials, rate schemas, head-only
absence, accepted bytes, a custom envelope controller and a byte stream, is the
Catalog of [Modeling APIs](modeling_apis.md).

## Selected decisions

| Area | v3 contract |
|---|---|
| Authoring | Root `values` block; scope/endpoint parameter lists; direct query/header placement; explicit mapping blocks can coexist with it. |
| Local references | A bare name means the current owner only. Ancestors are explicitly qualified. |
| Routes | Relative path templates in a `path` clause, no leading/trailing slash. DNS remains a typed label list. |
| Key inheritance | A plain introduction never overwrites; `replace`, `when_present`, `append`, `remove` and family `replace`/`off` are explicit. |
| Value operations | `set_x`, exact `replace_x(Option<T>)`, conditional `set_x_if_some`, `omit_x` and retained `reset_x` have different names. |
| Conversion | Identity, `&str -> String`, `&String -> String`; no blanket `Into`, numeric widening or recursive container conversion. |
| Common controls | The public, unsealed `RequestControls` trait; execution authority stays in core. |
| Time | Finite timeout selection/reset; independent non-widenable caps; one deadline for a whole traversal. |
| Rates | Reusable window sets hold numbers; explicit policy IDs and ordered key labels determine state sharing. |
| Expected absence | `absent on 404;` is an Option-oriented, head-only outcome with stated capability restrictions. |
| Pages | One-page `.await`; separate complete/prefix/lazy terminals; cancelling a started page fetch terminates the traversal. |
| Streams | Acquisition is not consumption; finite configurable lease; pending-read idle accounting; explicit completion. |
| Credential storage | The latest-started explicit installation intent supersedes earlier pending intents (`StoreOutcome::Superseded`). |
| Runtime endpoints | The same owner projection and symbolic operations; no new origin, credential-source or quota-removal authority. |
| Version scope | V3 only: one canonical grammar and runtime, with no older-version aliases or migration tool. |

## What v3 deliberately separates

```text
Rust payload type               != Concord presence (?)
presence                        != initializer (= V, #[default])
collection value                != query/header occurrence count (each)
request representation          != response representation
representation                  != caller delivery lifecycle (await, pages, stream)
status acceptance               != response conversion
expected absence (absent on)    != buffered NotFound<R> adaptation
mechanical body replay          != remote effect class
policy retry                    != auth recovery != transport-private retry
page operation                  != traversal aggregate
stream acquisition              != stream EOF
installation intent epoch       != installed credential version
public protocol data            != non-sensitive data
inspection                      != mutation authority
```

## Scope

V3 retains typed buffered responses, byte streams, framed record streams,
ordinary and custom request/response representations, custom authentication
providers, endpoint-backed credential storage, rate limiting, policy retries,
accepted response metadata, bounded status bodies, explicit accepted-byte
terminals, unbound requests and validated runtime endpoints, with JSON,
NDJSON, CSV, server-sent events, multipart, OAuth2 client credentials, TLS, HTTP/2 and gzip/Brotli/
deflate decompression feature paths.

One closed invocation is the unit of semantic validation. Support types, DTOs,
codecs, controllers and application code belong in ordinary Rust modules.

## Negative space

V3 does not add arbitrary absolute runtime origins, raw or pre-encoded URL
paths, dynamic key expressions inside the DSL, arbitrary native request/response
middleware, redirects, cookies or caching, `CONNECT`/`TRACE`, WebSockets or
upgrades, a general status-callback DSL, implicit `Option`/`Vec` wire
inference, generic query-object serialization, optional path/DNS structure,
body defaults, infinite timeouts or streams, background pagination prefetch,
implicit stream reconnection, a distributed-limiter backend, or independently
validated macro fragments. These are scope decisions; an application needing
them uses a lower-level HTTP client or a separately designed product boundary.
