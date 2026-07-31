# Mental model

- **Audience:** API authors, callers, reviewers, and extension authors
- **Purpose:** explanation
- **Normative references:** [DSL specification](spec.md), [runtime model](runtime.md), [design invariants](design_invariants.md)

## Concord is one semantic model with two public projections

The DSL and generated Rust API are not independent products. They are two views
of one resolved contract:

```text
declaration facts
→ normalized semantic model
→ generated Rust surface
→ typed runtime plan
```

A good declaration must generate desirable usage. A generated method must have
a meaning that is visible in the declaration.

## Client, scope, endpoint

```text
client
└── scope
    └── nested scope
        └── endpoint leaf
```

### Client

The client is the contract root and built runtime owner. It owns the fixed base,
client values, credential sources, reusable rate profiles, client-level
attachments, and the immutable runtime snapshot.

### Scope

A scope is an immutable selected domain context. It may own typed values,
inherited request semantics, policy attachments, child scopes, endpoint
leaves, generated modules, and generated navigation methods.

A scope is not merely a path prefix or visual folder. Domain containment and
wire path often align, but remain separate decisions.

### Endpoint leaf

An endpoint is one typed domain operation plus its local HTTP and policy delta.
The signature is the primary semantic statement. Its clauses explain how the
operation maps to the wire and runtime.

## Four different actions

### Declaration

Creates a concept: a client, scope, endpoint, value, credential source, or rate
profile.

### Attachment

Applies semantics at a client, scope, or endpoint: path, query, headers, auth,
timeout, or rate policy.

### Binding

Connects a reusable or runtime-controlled field to a visible value. Examples
include rate-profile formals and pagination controller fields.

### Execution

Starts managed work by polling a request terminal. Construction, scope
selection, endpoint construction, and planning remain inert.

Collapsing these actions into one generic configuration mechanism would make
ownership and timing ambiguous.

## Values, absence, defaults, and snapshots

| Declaration property | Generated controls |
|---|---|
| required | constructor argument or required client builder setter |
| `Option<T>` | `set_*`, `clear_*` |
| defaulted | `set_*`, `reset_*` |
| `Option<T>` with a default | `set_*`, `clear_*`, `reset_*` |

Absence and default are different. Clearing selects semantic absence. Resetting
restores the declaration default retained by the current immutable snapshot.
Defaults are evaluated when their owner snapshot is created, not on every
attempt or page.

## Public and private request worlds

```text
public values
→ logical URL and public headers
→ plans, hooks, rate dimensions, safe observation

credential material
→ private placement after limiter admission
→ native request owned by core
```

A credential-free plan may still contain sensitive application data such as
account or tenant identifiers. “Public” means permitted in public request
shaping, not universally safe to log.

## Capability-shaped generated API

The generated method set reflects declared and trait-proven capabilities:

- pagination exists only for declared paginated endpoints;
- raw accepted response bytes exist only for buffered or discard modes;
- credential-storage terminals exist only for source endpoints;
- applications cannot forge generated endpoints through the sealed trait.

The absence of a method is part of the contract.

## One executor is a consequence

Direct methods, explicit endpoints, runtime endpoints, retries, authentication
recovery, pagination pages, and response-byte terminals must preserve the same
behavior. One managed executor prevents each surface from becoming a separate
implementation of deadlines, limiting, credentials, response handling,
cancellation, errors, and completion.

## Semantic extension, not native escape

Custom types extend a role—body, response, credential source, controller,
limiter, classifier, or hook—through bounded semantic data. Runtime-defined
endpoints escape compile-time generation but still use the same semantic model
and executor. The escape hatch leaves the macro, not the guarantees.

## Example

<!-- concord-source: crates/concord-examples/src/getting_started/domain_tree.rs#domain_tree_contract -->
```concord
api! {
    client CatalogApi {
        base "http://catalog.example.test/";
        vars {
            tenant: String;
        }
        path ["api"];
        query { "tenant" = vars.tenant; }
    }

    scope regional(region: String) {
        host [scope.region, "api"];
        path ["v1", "catalog"];

        GET item_summaries(
            category: Option<String>,
            offset: u64 = 0,
            count: std::num::NonZeroU32 = DEFAULT_PAGE_SIZE,
        ) -> Json<Vec<ItemSummary>> {
            path ["items"];
            query {
                "category" = ep.category;
                "offset" = ep.offset;
                "count" = ep.count;
            }
            paginate concord::pagination::OffsetPagination {
                offset: bind ep.offset,
                page_size: bind ep.count,
            }
        }
    }
}
```

The tree communicates ownership; the signature communicates the operation;
the clauses communicate the local wire mapping; the generated use follows the
same structure.

## Continue

- [Modeling APIs](modeling_apis.md)
- [Generated client](generated_client.md)
- [Execution policy](execution_policy.md)
