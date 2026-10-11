# Migration to Concord v2.1 (historical)

**Historical:** this is the v2.0 to v2.1 guide, kept for reference. It
describes the retired v2.1 surface, not the current product. To migrate
Concord v2.1 code to v3, read [migration_v3.md](migration_v3.md) and use the
`migrate-v3` tool.

This guide targets the supplied v2.0.0 baseline at `f7464c5` directly. Some
retired forms originated in its earlier v1 language lineage; the headings below
retain “v1” only when distinguishing those forms. Do not publish an intermediate
compatibility layer first.

## Migration principles

- preserve domain meaning, not surface spelling;
- do not guess where v1 inferred semantics from `Option<T>`, `Vec<T>`, response wrappers, or hooks;
- migrate one semantic slice at a time with old/new effective-contract snapshots;
- remove superseded public names after call sites and docs move;
- do not retain two canonical languages as a compromise.

## Source structure

### Client containment

v1:

```concord-pseudocode
api! {
    client CatalogApi { /* root */ }
    scope regional(region: Region) { /* ... */ }
}
```

v2.1:

```concord-pseudocode
api! {
    client CatalogApi {
        /* root */
        scope regional(region: Region) { /* ... */ }
    }
}
```

Move every root scope/endpoint inside the client. Generated modules/navigation and source containment become isomorphic.

### `vars` and owner references

| v1 | v2.1 |
|---|---|
| `vars { ... }` | `values { ... }` |
| `vars.tenant` | `client.tenant` |
| `scope.region` | `regional.region` (exact scope owner) |
| `ep.id` | `endpoint.id` |

Do not mechanically replace `scope.` without resolving the actual owner.

## Values and optionality

v1 interpreted exact `Option<T>` as a Concord-optional inner `T`. v2.1 treats Rust types as opaque.

| v1 intent | v2.1 |
|---|---|
| optional category written `category: Option<Category>` | `category?: Category` |
| required domain optional payload | `category: Option<Category>` |
| optional slot around optional payload | `category?: Option<Category>` |

Every old `Option<T>` declaration requires semantic review. A source rewriter must not guess.

### Defaults

| v1 | v2.1 |
|---|---|
| `x: T = default` | `#[default] x: T` |
| `x: T = CONST` | unchanged stable initializer form |

Review owner-snapshot timing: client build, scope construction, or endpoint construction. `reset_x()` restores the retained evaluated value.

### Generated controls

| v1 | v2.1 |
|---|---|
| optional `.set_x(T)` | `.set_x(Option<T>)` |
| `.clear_x()` | `.omit_x()` |
| v2.0 `.with_x(T)` | `.set_x(Some(value))`; convenience removed while human evidence is unavailable |
| `.reset_x()` | retained for initialized slots |

For nested optional values, update exact types rather than flattening.

## Public shaping

### `fmt`

| v1 | v2.1 |
|---|---|
| `fmt[...]` | `concat [...]` |

`concat` produces one atom and cannot contain optional values or repetition.

### Canonical atom encoding

v1 uses `Display` broadly. v2.1 requires:

- `RequestAtom` on domain types with one stable protocol atom; or
- an explicit placement adapter, e.g. `endpoint.id via Base32Id`;
- `DisplayAtom` only as an explicit compatibility extra.

Every domain type used in host/path/query/header shaping must be reviewed. Changing human display must no longer silently alter wire behavior.

### Host/subdomain

| v1 | v2.1 |
|---|---|
| `host [scope.region, "api"]` | parent `subdomain ["api"]`; child `subdomain [regional.region]` |
| `host off` | `subdomain off` |

Ordinary v2.1 subdomain clauses prepend local labels to inherited generated labels. Remove repeated parent labels.

### Path

Migrate to `path`, `path replace`, or `path off`. Review any v1 raw/dot/slash behavior against the one-atom/one-segment rule.

## Query and headers

v1 ordinary assignment remains assignment, but optional and collection behavior must be reviewed.

v2.1 forms:

```concord-pseudocode
"key" = value;
when_present "key" = optional_value;
append "key" = value;
append "key" = each sequence;
remove "key";
```

Migration rules:

- v1 optional assignment that intentionally clears inherited values → ordinary assignment;
- optional override that should inherit when absent → `when_present`;
- repeated query/header from v1 `Vec<T>` inference → explicit `each`;
- additional occurrence → `append`;
- scalar collection representation → explicit adapter such as `CommaSeparated`;
- dynamic query/header names remain unsupported.

Do not preserve `?=`, `+=`, or `...` as public aliases.

The v2.1 exact optional-control surface looked like this (historical; the v3
spelling uses `set_x(v)` for payloads and `replace_x(Option<T>)` for exact
replacement):

```text
    let _: reference_api::StatusRequest = client
        .status()
        .set_verbose(Some(true))
        .omit_verbose()
        .reset_verbose()
        .set_tags(Some(vec!["stable", "documented"]))
        .omit_tags()
        .reset_tags();
```

## Request and response representation

v1:

```concord-pseudocode
POST create(#[body] body: Json<Create>) -> Json<Item>;
GET rows() -> Records<NdJson<Row>>;
GET bytes() -> Stream<OctetStream>;
```

v2.1:

```concord-pseudocode
client Api {
    request Json;
    response Json;

    POST create(body input: Create) -> Item;

    scope records() {
        response NdJson;
        GET rows() -> Vec<Row>;
        GET watch() -> ItemStream<Row>;
    }

    GET bytes() -> ByteStream via Binary;
}
```

Review independently:

- exact caller body input;
- exact awaited output;
- stable representation owner;
- complete versus lazy caller lifecycle;
- empty versus discard versus JSON null;
- status acceptance.

Do not translate wrapper aliases mechanically into more wrapper aliases.

## Status

Move accepted status from `ResponseBodySpec::accepts_status` or custom response wrappers to endpoint static data:

```concord-pseudocode
status success;
status [200, 201, 404];
```

Custom response representations now convert only an accepted response. Review auth-challenge overlap and retry behavior.

## Effect/repeatability

v1/v2.0 `repeatability idempotent|never` becomes:

```concord-pseudocode
effect safe;
effect idempotent;
effect non_idempotent;
```

Review every explicit declaration. Defaults derive from method. Pagination requires `safe`.

## Request bodies

| v1 | v2.1 |
|---|---|
| `#[body] body: Json<T>` | `body input: T` with inherited `request Json` or `via Json` |
| wrapper-associated input | exact caller input |
| optional body inferred from payload | explicit `body input?: T` |

Body defaults remain unsupported. Preserve per-page preparation and within-page replay; do not introduce aggregate prepared-body reuse during migration.

## Responses and terminals

| v1 | v2.1 |
|---|---|
| `ResponseBodySpec` | `ResponseRepresentation<Output>` target trait |
| `Json<T>` output wrapper | exact `T` plus representation default/override |
| `response_bytes()` | endpoint `expose accepted_bytes;` plus `accepted_bytes()` |
| decoder-bearing stream type | `ByteStream` or `ItemStream<T>` |

Do not retain broad aliases indefinitely.

## Timeout

- remove `timeout off` and generated disable methods;
- choose a finite contract timeout;
- make one absolute deadline cover all attempts/pages/stages;
- keep post-return stream read-idle/byte limits separate.

## Authentication

Preserve the source/slot/private-placement model. Migrate:

- typed secret declarations to required protected `secret name;`;
- repeated leaf auth clauses into one grouped `auth { ... }` block;
- challenge policy remains attribution, not automatic refresh;
- endpoint-backed source path becomes client-root-relative semantic path;
- storage terminal returns unit and ordinary await does not store.

Do not add direct secret-to-header shortcuts.

## Rate limits

v1:

```concord-pseudocode
rate_limits {
    account_reads(account) {
        by [host, account];
        100 / 1m;
    }
}
rate_limit use account_reads { account = scope.account; }
```

v2.1:

```concord-pseudocode
rate_limits {
    policy account_reads(account) {
        id "service.account.read";
        key [origin, endpoint, account];
        cost 1;
        allow 100 per 1m;
    }
}
rate_limit account_reads(account = account_scope.account);
```

Review stable IDs, canonical `RateKeyPart`, dimensions, cost, conjunctive windows, additive attachments, and explicit limiter selection. Remove feature-driven implicit local limiter installation.

## Pagination

v1:

```concord-pseudocode
paginate OffsetPagination {
    offset: bind ep.offset,
    page_size: bind ep.count,
}
```

v2.1:

```concord-pseudocode
paginate via OffsetPages {
    offset -> endpoint.offset;
    page_size -> endpoint.count;
}
```

Review:

- endpoint effect becomes `safe`;
- one-page exact output;
- controller-owned item extraction/continuation;
- declared response-header interests;
- page-live bindings and rate-key conflicts;
- one aggregate deadline and hard send ceiling;
- `PageLimit::Prefix` versus `CompleteWithin`;
- no lazy page stream.

## Hooks and observation

Remove broad pre-send and response-head veto hooks. Map uses to:

- domain/value validation;
- request/body/auth declarations;
- retry classifier;
- rate limiter feedback;
- endpoint status/response validation;
- application workflow;
- read-only `OperationObserver`;
- request planning or deterministic test transport.

A current-result-failing rate observer must become non-fallible future-admission feedback.

## Planning and names

| v1 | v2.1 |
|---|---|
| `SafeRequestPlan` | `RequestPlan` with explicit credential-free/redaction-aware guarantee |
| broad raw debug | sanitized structured views |
| current endpoint source names as identity | explicit stable IDs where operational continuity matters |

## Runtime endpoints

Migrate to exact local patch operations, `RuntimeValue::sensitive`, explicit representation/status/effect/timeout, and fallible `try_bind`. Remove any native/absolute-origin escape.

## Migration sequence

1. Archive v1 public/effective-contract/fixture baselines.
2. Introduce canonical IR and descriptions without behavior change.
3. Implement value axes, owner references, atoms, and query/header words.
4. Implement representation pairs, exact body/output, status, effect, streams.
5. Tighten executor, observer, retry/rate/server-delay, auth, pagination.
6. Migrate extras and runtime endpoints.
7. Migrate examples, docs, UI fixtures, and public facade.
8. Remove v1 aliases/parser branches.
9. Run every release gate and full repository verification.

## Search-based completion checks

Active code/docs should contain no canonical use of:

```text
vars.
scope.
ep.
host [
fmt[
#[body]
ResponseBodySpec
response_bytes
SafeRequestPlan
timeout off
clear_
rate_limit use
Json<T> in endpoint result position
implicit Vec repetition
```

Migration-guide quotations are the only exception.
