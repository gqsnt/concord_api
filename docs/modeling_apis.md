# Modeling APIs in Concord v2.1

Concord should make the domain tree primary while keeping every consequential wire, security, effect, and resource fact inspectable.

## Start from persona tasks

Before declaring a large API, write representative tasks:

- build a client for one deployment/security context;
- select one stable domain context;
- call a common endpoint;
- configure an optional/defaulted value;
- handle one expected domain outcome and one uncertain failure;
- inspect a request without executing it;
- add one exceptional binary/record endpoint;
- add pagination without changing caller understanding of one page;
- identify every endpoint affected by a parent change.

## Choose stable owners

### Client

Owns facts stable for the complete contract/security context:

- fixed logical base;
- root finite timeout;
- public client values;
- credential source graph;
- reusable rate-policy definitions;
- common request/response representations;
- root public/private patches.

### Scope

Owns one selected domain context and shared subtree behavior:

- required/optional/defaulted context values;
- subdomain/path and public patches;
- auth/rate/timeout deltas;
- common representation for a coherent subtree;
- child scopes/endpoints.

A scope is not a file-system folder. It should represent selected context, shared contract, or meaningful domain grouping.

### Endpoint

Owns one operation:

- exact required inputs and body;
- exact awaited output;
- local representation exception;
- stable endpoint ID;
- local route/query/header changes;
- accepted status;
- effect class;
- local auth/rate/timeout changes;
- pagination and lower-level terminal capabilities.

## Representation defaults

Use defaults when a representation is stable across a coherent owner:

```concord-pseudocode
client Api {
    request Json;
    response Json;

    GET item(id: Id) -> Item;
    POST create(body input: Create) -> Item;
}
```

Use an inline exception when only one body or result differs:

```concord-pseudocode
POST login(body input: Login via Form) -> Token;
GET download(id: Id) -> ByteStream via Binary;
```

Fence heterogeneous branches:

```concord-pseudocode
scope assets() {
    request explicit;
    response explicit;

    GET metadata(id: Id) -> Metadata via Json;
    GET bytes(id: Id) -> ByteStream via Binary;
}
```

`explicit` means descendants must choose a representation locally. It does not mean raw, disabled, empty, or unbounded.

## Value design

Use the written Rust type for domain meaning and the DSL markers for Concord semantics:

```rust
mode: Option<Mode>        // required domain decision
mode?: Mode               // optional slot
mode?: Option<Mode>       // optional slot around an optional payload
#[default]
page_size: PageSize       // retained default
```

Do not select `Option<T>` or `Vec<T>` merely to trigger wire behavior.

## Public atom design

A domain type used in URL/query/header shaping should either own one canonical protocol atom through `RequestAtom` or use an explicit placement adapter:

```concord-pseudocode
path [endpoint.id via Base32Id];
query { "at" = endpoint.time via Rfc3339; }
```

Use a scalar adapter for one joined collection and `each` for repeated occurrences:

```rust
"fields" = endpoint.fields via CommaSeparated;
append "tag" = each endpoint.tags via TagSlug;
```

## Status and effect

Keep them explicit when non-default:

```concord-pseudocode
status [200, 201];
effect idempotent;
```

`safe`, `idempotent`, and `non_idempotent` describe the requested remote effect. A body being replayable or an idempotency header existing does not infer the class.

Ordinary pagination requires `safe`, not merely `idempotent`, because pagination automates several remote operations under one all-or-error terminal.

## Authentication

Authentication is a private source/slot/placement/recovery model, not a header shortcut. Declare credential sources once, attach purpose-specific placements, and attribute at most one recoverable challenge.

An unavailable credential for a declared placement fails before dispatch. It is never silently omitted.

## Rate limits

Declare provider quota semantics in the contract and select enforcement in the application. Policies are additive; a child cannot disable an ancestor provider limit. Use explicit stable IDs and canonical key parts.

## Pagination

The endpoint returns one complete page. The controller owns item extraction and continuation:

```concord-pseudocode
GET items(cursor?: Cursor) -> ItemPage {
    effect safe;
    paginate via CursorPages {
        cursor -> endpoint.cursor;
    }
}
```

The controller may see only the decoded page plus the bounded response headers it declared. Cross-origin continuation is rejected.

## Review a parent change

Any change to a parent representation, query/header/auth/rate patch, timeout, or sensitivity can affect many leaves. Review the expanded semantic diff rather than only the textual edit.

A parent default is justified only when:

- it belongs to a coherent subtree;
- exceptions remain local;
- the effective owner is shown beside each endpoint in generated docs;
- moving an endpoint reports every effective change;
- an `explicit` fence can stop valid-but-wrong inheritance.
