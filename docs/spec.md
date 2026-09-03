# Concord v2 DSL specification

- **Audience:** API authors, reviewers, and macro maintainers
- **Status:** normative for declaration syntax and compile-time semantics

Concord accepts exactly one v2 client per `api!` invocation. All scopes and
endpoints are lexically contained by that client. Selected retired forms are
recognized only to produce migration diagnostics; they do not select another
semantic mode. See the [migration guide](migration_v1_to_v2.md).
The compile-checked [catalog v2 contract](../crates/concord-examples/src/contracts/catalog_v2.rs)
shows the complete declaration and representative direct and unbound usage in one file.

## Grammar overview

```text
api             = api! { client }
client          = attributes client TypeIdent { client_item* }
client_item     = base | timeout | values | credentials | rate_limits
                | attachment | scope | endpoint
scope           = attributes scope ident ( parameters? ) { scope_item* }
scope_item      = attachment | scope | endpoint
endpoint        = attributes method ident ( parameters? )
                  -> Type using Type ( ; | { endpoint_item* } )
value_parameter = attributes ident [?] : Type [= stable_value]
body_parameter  = body ident : Type using Type
method          = GET | POST | PUT | PATCH | DELETE | HEAD | OPTIONS
                | METHOD("TOKEN")
```

`base` is required and client-only. The root must also select `timeout D;` or
`timeout off;`. A parameter list may contain ordinary values and at most one
body parameter in source order.

Every Rust `Type` is parsed as arbitrary valid Rust type syntax and retained as
opaque tokens. The macro never recognizes a path such as `Option`, `Vec`, an
alias, or a qualified spelling to assign language semantics.

## Documentation and attributes

Rust `///` documentation is contract data and appears in generated Rustdoc and
descriptions. Supported non-doc attributes are:

- `#[default]` on ordinary values;
- `#[redact]` on ordinary public values;
- `#[deprecated(note = "...")]` on clients, scopes, and endpoints.

Other attributes are declaration errors. `#[default_with]` and value-level
deprecation are not part of v2.

## Owners and names

Ordinary references always identify the owner: `client.tenant`,
`regional.region`, or `endpoint.item_id`. A scope reference uses the exact
visible source identifier. Reusing one scope identifier in an ancestor chain is
an error; disjoint sibling branches may reuse it.

Body inputs are not public-expression values. Secrets appear only as
`secret.name` in credential-source declarations. Credential names appear only
inside authentication declarations.

Resolution binds references to stable internal owner and value identities.
Every generated type, method, control, terminal, storage method, unbound method,
and hidden source-derived bridge name participates in collision analysis. A
collision identifies both declarations and the generated symbol; convenience
methods are never silently dropped.

## Values: exact type law

`?` adds exactly one Concord-owned outer presence layer. The written payload
type remains unchanged.

| Declaration | Effective stored type | Required input? |
|---|---|---|
| `x: T` | `T` | yes, unless defaulted |
| `x?: T` | `Option<T>` | no |
| `x: Option<T>` | `Option<T>` | yes, unless defaulted |
| `x?: Option<T>` | `Option<Option<T>>` | no |
| `x: Vec<T>` | `Vec<T>` | yes, unless defaulted |
| `x?: Vec<T>` | `Option<Vec<T>>` | no |

A required `Option<T>` asks the caller for a domain decision, where `None` is a
valid payload. For `x?: Option<T>`, outer `None` omits the decision,
`Some(None)` explicitly clears it, and `Some(Some(v))` sets it. Storage,
setters, placement, reflection, and diagnostics never flatten these states.

## Defaults and controls

`#[default]` evaluates `<T as Default>::default()` for the payload type. A
stable initializer may instead be a string, character, Boolean, or signed
numeric literal, or a simple const/static path. Calls, methods, macros, blocks,
closures, indexing, collection construction, and arbitrary Rust expressions
are rejected. Attribute default and explicit initializer are mutually
exclusive.

The initializer is applied before the outer presence layer. Thus a defaulted
optional value begins as `Some(default)`, and a defaulted optional
`Option<T>` begins as `Some(None)`.

Defaults are evaluated once at the narrowest owner boundary: successful client
build, scope creation, or endpoint intent construction. Retried attempts,
recovery, and pages reuse the retained value. A snapshot `reset_x()` restores
that retained value without reevaluation or `Clone`; a builder reset removes
the override so the next build evaluates once.

Generated controls consume `self`:

| Value kind | Controls |
|---|---|
| required, no default client value | `set_x(T)`; build reports omission |
| required, no default scope/endpoint value | positional constructor argument |
| optional `x?: T` | `set_x(Option<T>)`, `with_x(T)`, `clear_x()` |
| required defaulted | `set_x(T)`, `reset_x()` |
| optional defaulted | optional controls plus `reset_x()` |

`set_x(Some(v))` equals `with_x(v)` and `set_x(None)` equals `clear_x()`.
Exact types are used; broad `Into<Option<T>>` setters are not generated.
Scope/endpoint constructor arguments contain required non-defaulted ordinary
values and body inputs in original source order.

## Public expressions and cardinality

The only public atoms are string literals, supported integer literals, direct
owner references, and `concat [atoms...]`. Nested concatenations flatten into
one bounded write. Calls, indexing, extended field chains, macros, closures,
blocks, conditions, matching, arithmetic, secrets, and credentials are rejected.

An outer optional slot is allowed directly only in omission-aware query or
header assignment. It is rejected in subdomain labels, path segments,
concatenation children, `accept`, and rate dimensions. A required payload whose
Rust type happens to be optional is not a Concord-optional slot; normal trait
requirements decide whether it can be formatted or iterated.

One-value query emission uses `"key" = endpoint.value;`. Repetition is explicit:
`"tag" = each endpoint.tags;`. `each` accepts only a direct owner reference and
iterates the payload by shared reference. An absent outer slot emits nothing;
iteration order is observable. Headers intentionally have no repeated-value
operator in v2.

URL and header atoms use bounded `Display`-style conversion followed by
context validation. Rate dimensions use the separate canonical-key contract.

## Fixed base and subdomains

The base must be an absolute HTTP(S) URL with a host and without user info,
query, or fragment. Its optional path is fixed. There is no per-request base or
authority override.

Subdomain operations are `subdomain [...]`, `subdomain replace [...]`, and
`subdomain off;`. Omission inherits. A normal local list prepends its labels to
the inherited generated labels; replace discards inherited generated labels;
off returns to the fixed base host. Each atom creates one ASCII DNS label.

Static labels are checked at expansion and dynamic labels during planning and
execution. Label and total-host bounds apply. Any effective subdomain shaping
requires a DNS-domain base; IP-literal bases are rejected.

## Path algebra

Path operations are `path [...]`, `path replace [...]`, and `path off;`.
Omission inherits; normal operation appends generated segments; replace starts
with no inherited generated segments; off removes all generated segments. The
fixed base path is never removed.

Each atom is one segment. Empty, dot, dot-dot, slash, backslash, and
pre-encoded values are rejected. A valid value is encoded exactly once. Segment
count, segment bytes, and total URL bytes are bounded.

## Query patch algebra

The effective query is an ordered sequence of pairs, never a map. Keys are
static strings with exact case-sensitive identity. Inherited state and local
statements are processed deterministically in source order.

- `"k" = emission;` removes all exact-key occurrences, then appends zero, one,
  or many emitted pairs. Absent optional assignment therefore removes inherited
  values.
- `"k" += emission;` appends emission without removal. Absent optional append
  changes nothing.
- `remove "k";` removes all occurrences.
- `query replace { ... }` starts empty.
- `query off;` produces an empty query and is exclusive at that site.

Repeated statements for one key are meaningful. Values are bounded before
retention. Public query shaping precedes private auth query placement, and a
public/private key collision fails before dispatch.

## Public header patch and media ownership

Header identity is ASCII case-insensitive. Assignment replaces all current
values and inserts one at the statement position; absent optional assignment
removes inherited values. `remove`, `headers replace`, and `headers off` mirror
the query concepts. There is no append operator.

Public headers reject host, authorization/proxy authorization, hop-by-hop and
transfer fields, cookie-owned fields, content length, content type, and accept.
`RequestBodySpec` exclusively owns request media and `Content-Type`.
`ResponseSpec` owns default `Accept`; an endpoint alone may write `accept atom;`
or `accept off;`.

## Request bodies and responses

A body parameter is written `body input: Input using Spec`. `Input` is exactly
what the caller supplies and `Spec` must implement
`RequestBodySpec<Input = Input>`. It may occur anywhere in the parameter list,
but cannot be optional, defaulted, redacted, publicly referenced, or controlled
after construction. An optional domain body is explicitly `Input = Option<T>`;
the spec owns its encoding.

An endpoint result is written `-> Output using Spec`. Await returns exactly
`Output`, and `Spec` must implement `ResponseSpec<Output = Output>`. Generated
endpoint-local assertions make either mismatch a compile error.

Every request supports await and `.response().await`. `expose accepted_bytes;`
adds `.accepted_bytes().await` only when the sealed response mode supports it.
It preserves challenge handling, status acceptance, negotiation, head limits,
and bounded acquisition while skipping declared conversion. Buffered mode uses
acquired bytes; discard may switch to bounded buffering; streaming is rejected.

Response acquisition is sealed to buffered, streaming, or discard. Receiving a
stream value does not mean the body was fully consumed, and no buffer terminal
may silently retain an unbounded stream.

## Methods and repeatability

First-class methods are GET, POST, PUT, PATCH, DELETE, HEAD, and OPTIONS.
`METHOD("TOKEN")` accepts another valid HTTP method token. CONNECT is always
rejected because Concord exposes no tunnel/upgrade semantics. TRACE is available
only as `METHOD("TRACE")` and carries its usual security concerns.

Endpoint-only `repeatability idempotent;` and `repeatability never;` override
the method default. GET, HEAD, PUT, DELETE, OPTIONS, and exact TRACE default to
idempotent; POST, PATCH, and unknown methods default to never. Idempotent grants
only semantic permission for another eligible attempt; it does not claim safety,
cacheability, server correctness, or automatic retry desirability.

## Credentials and authentication

Credential blocks declare protected inputs and sources. A `secret name;`
generates a required `SecretString` builder input. Sources include API key,
bearer, Basic, endpoint-backed acquisition, and custom providers. Endpoint
sources resolve to stable endpoint IDs and participate in cycle detection.
Optional secrets/providers are not supported.

Authentication is grouped as `auth { placements... }`, `auth replace { ... }`,
or exclusive `auth off;`. Normal blocks add ordered placements; replace starts
without inherited placements. Bearer, Basic, exact/scheme authorization,
private header, and private query placement are supported. Duplicate targets
and incompatible authorization forms are errors.

Recovery is explicit as `recover on unauthorized` or
`recover on unauthorized_or_forbidden`; omission means none. Credential and
secret material cannot occur in public expressions or observations.

## Timeouts

Durations are positive integers followed by `ms`, `s`, `m`, or `h` and are
validated for overflow. Descendants inherit by omission and may select a
duration or `timeout off;`. Every request exposes `.timeout(duration)`,
`.disable_timeout()`, and `.inherit_timeout()`; the last restores the resolved
endpoint selection.

## Pagination declarations

Pagination uses `paginate with Controller { fields... }`. A directional field
`controller_field -> endpoint.value;` grants the adapter authority to update
exactly that endpoint-owned slot between pages. A stable initializer
`controller_field = value;` copies initial state without mutation authority.
Fields and live destinations are unique and source-ordered.

Live destinations cannot be client/scope values, bodies, secrets, credentials,
or arbitrary request state. A pagination-controlled slot cannot be a rate-key
dimension in v2. Callers may configure ordinary optional/defaulted controls
before pagination begins; the controller owns bound slots after handoff.

## Rate limits

Client-owned profiles declare formals and do not capture lexical values. A
profile body has optional explicit `id`, `key [dimensions...]`, positive `cost`
(default 1), and one or more positive `allow capacity per duration;` windows.
Each attachment binds every formal exactly once and may override cost. Anonymous
attachments declare the same facts inline.

Policies add across client and scopes; descendants cannot disable them. Built-in
dimensions are final logical `host` and stable `endpoint` ID. Other dimensions
are total direct owner references. Outer optional slots, concatenation, secrets,
credentials, and pagination-mutated slots are rejected.

`RateLimitKeyValue` writes deterministic, bounded, type/length-framed bytes.
Standard primitive and structural implementations distinguish variants and
boundaries, so tuple-like dimensions cannot concatenate ambiguously. Values are
never exposed through plans or ordinary evidence.

IDs are explicit or deterministic/rename-sensitive. Reused IDs require
compatible ordered dimension schemas and windows; duplicate effective use is an
error. Any client with effective rate policy requires an explicitly configured
limiter.

## Reflection and unbound intents

`Client::contract()`, request/intent `.description()`, and credential-free plans
derive from the canonical IR. Descriptions expose docs, owners, opaque type
spellings, presence/default/observation, local and effective operations with
origins, body/response contracts, auth categories, timeouts, repeatability,
rate schemas, pagination bindings, capabilities, and identity stability—never
runtime values or credential material.

`Client::unbound()` generates the same scope/endpoint tree without client
runtime or client values. `client.request(intent)` binds it and
`client.plan(&intent)` plans it. Direct and unbound-bound paths are semantically
equivalent.

## Diagnostics and limits

Diagnostics target retired syntax, unsupported attributes/clauses, owner and
name errors, optionality in total contexts, invalid `each`, patch conflicts,
reserved media, body/response mismatches, CONNECT, pagination authority, rate
schemas, accepted-byte mode, and generated-name collisions. They identify the
declaration and rule without runtime values.

All static collections and strings are bounded. The same limits continue at
runtime for dynamic formatting, route/head preparation, response acquisition,
attempts, pages, items, cycle state, canonical keys, and evidence.
