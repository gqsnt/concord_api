# Concord v2.1 DSL specification

This document is normative for authored language meaning. It specifies the implementation target, not yet a public compatibility freeze. Private compiler representations may differ; generated/runtime behavior may not.

# 1. Lexical and structural model

## 1.1 One closed client tree

```text
api_decl     := api! { client_decl }

client_decl  := attrs client TypeIdent {
                    client_preamble*
                    child+
                }

child        := scope_decl | endpoint_decl

scope_decl   := attrs scope ident ( parameter_list? ) {
                    scope_preamble*
                    child+
                }

endpoint_decl:= attrs method ident ( endpoint_parameter_list? )
                -> RustType result_representation?
                ( ; | { endpoint_clause* } )
```

Rules:

- exactly one client per invocation;
- every scope and endpoint is lexically inside that client;
- a client/scope configuration preamble precedes the first child;
- after the first child, only child declarations are accepted;
- sibling child names are unique;
- a scope must contain at least one child;
- one visible root-to-leaf chain cannot contain the same canonical scope name twice;
- sibling branches may reuse a scope name;
- Rust types/paths are parsed opaquely and resolved by Rust after expansion.

## 1.2 Supported attributes

Supported where the generated projection has meaning:

```text
/// and #[doc = ...]
#[deprecated(...)]
#[default]
#[sensitive]
```

- `#[default]` applies only to ordinary public values;
- `#[sensitive]` applies only to ordinary public values;
- body/content and credential material have stronger dedicated information classes;
- unknown attributes are rejected;
- declaration-level `#[cfg]` remains unsupported; condition the containing module or complete `api!` invocation.

## 1.3 Clause multiplicity

At one owner:

- client: exactly one `base`, exactly one finite root `timeout`;
- client: at most one `values`, `credentials`, and `rate_limits` block;
- client/scope: at most one local `request`, `response`, `subdomain`, `path`, `query`, `headers`, and `auth` family declaration;
- client/scope/endpoint: rate-limit attachments may repeat because they are additive;
- endpoint: at most one `id`, `status`, `effect`, pagination declaration, and each exposed terminal;
- empty blocks are rejected when omission or `off` communicates intent more clearly.

Scalar clause order inside a preamble is semantically irrelevant. Order inside query/header/auth operation lists and rate windows is retained where defined.

# 2. Values, presence, defaults, and sensitivity

```text
values_block := values { value_decl* }
value_decl   := attrs ident optional_mark? : RustType initializer? ;
parameter    := attrs ident optional_mark? : RustType initializer?
optional_mark:= ?
initializer  := = stable_value
stable_value:= string | char | bool | signed_integer | ConstPath
```

## 2.1 Exact type law

`?` adds one outer Concord presence layer. No other type spelling has DSL meaning.

| Source | Effective type |
|---|---|
| `x: T` | `T` |
| `x?: T` | `Option<T>` |
| `x: Option<T>` | `Option<T>` required as payload |
| `x?: Option<T>` | `Option<Option<T>>` |
| `x: Vec<T>` | `Vec<T>` |
| `x?: Vec<T>` | `Option<Vec<T>>` |

The macro must not detect `Option`, `Vec`, stream types, aliases, qualified names, or familiar standard types.

## 2.2 Initialization

```rust
#[default]
x: T;

x: T = CONSTANT;

#[default]
x?: T;
```

- `#[default]` means `T::default()` on the exact written payload type;
- `#[default]` and `= value` cannot coexist;
- an optional initialized value starts as outer `Some(initial)`;
- defaults evaluate once when the narrowest owner snapshot is created;
- reset restores the retained result and does not reevaluate;
- body parameters cannot have defaults.

## 2.3 Sensitivity

`#[sensitive]` marks public protocol data whose automatic observation must be redacted. It does not hide the value from the server/network path and does not make it a secret or credential.

Sensitivity propagates through `concat`, adapters, and every `each` item to the complete emitted atom.

# 3. Representation ownership

## 3.1 Directional defaults

Representation defaults are valid at client and scope owners. `explicit` fences are valid only at scope owners:

```text
request RustType ;
response RustType ;
request explicit ;
response explicit ;
```

- `request R;` establishes an inherited request-body representation default;
- `response R;` establishes an inherited response representation default;
- `request explicit;` and `response explicit;` stop inheritance across that owner;
- `explicit` is an inheritance fence, not raw/disabled/empty behavior;
- request and response directions are independent;
- there is no combined `format R` shorthand.

Resolution for one direction:

```text
inline body/result override
else nearest enclosing scope default not blocked by explicit
else client default not blocked by explicit
else unresolved
```

A request representation is required only for an endpoint with a present body slot. A response representation is required for every endpoint.

## 3.2 Inline exceptions

```text
body_parameter       := attrs body ident optional_mark? : RustType [ via RustType ]
result_representation:= via RustType
```

Examples:

```concord-pseudocode
POST login(body input: Login via Form) -> Token;
GET download(id: Id) -> ByteStream via Binary;
```

The endpoint body/result types remain exact caller-visible types. `via` selects only the representation for that direction.

## 3.3 Generic pair obligations

The generated code requires conceptually:

```rust
EffectiveRequestRepresentation: RequestRepresentation<Input>
EffectiveResponseRepresentation: ResponseRepresentation<Output>
```

The macro does not infer trait implementations or delivery from token spelling. It emits targeted Rust obligations pointing to the endpoint and representation owner.

# 4. Public value references and atom expressions

```text
value_ref   := client . ident
             | scope_ident . ident
             | endpoint . ident

atom_source:= string | char | bool | signed_integer | value_ref
atom_expr  := atom_source [ via RustType ]
             | concat [ atom_expr (, atom_expr)* ,? ]

emission   := atom_expr
             | each value_ref [ via RustType ]
```

## 4.1 Owner references

- `client.x` refers only to a client public value;
- `scope_name.x` refers only to that exact visible scope owner;
- `endpoint.x` refers to the endpoint ordinary/body slot;
- credentials have no public expression namespace;
- no nearest-scope search, `vars`, `ep`, `self`, or `parent` shorthand exists.

## 4.2 Atoms

Default scalar use requires `T: RequestAtom`. `via Encoder` requires a purpose-specific atom adapter compatible with the exact source type.

One atom expression produces one logical scalar. Context code subsequently validates/encodes it as one DNS label, path segment, query value, or header value.

Optional values are illegal in total atom positions such as `concat`, subdomain, path, and rate dimensions. A surrounding query/header operation may own optional absence.

## 4.3 `concat`

`concat` produces one atom. Nested concatenation may normalize into one sequence. It cannot contain `each` or outer-optional values.

## 4.4 Ordered repetition

`each` is valid only in query/header emission. It requires `OrderedValues` and makes zero-or-many occurrence cardinality explicit. The collection type itself has no query/header meaning.

# 5. Fixed base, subdomain, and path

## 5.1 Base

```text
base_clause := base string_literal ;
```

The literal must be one absolute HTTP(S) URL with host and without userinfo, query, or fragment. The base owns the logical trust origin and fixed base-path prefix.

No caller, scope, endpoint, or runtime endpoint may replace it with an arbitrary absolute URL. A configured proxy changes transport routing only. Redirects remain disabled.

## 5.2 Subdomain

```text
subdomain_clause := subdomain atom_list ;
                  | subdomain replace atom_list ;
                  | subdomain off ;
atom_list        := [ atom_expr (, atom_expr)* ,? ]
```

- ordinary form prepends local labels to inherited generated labels;
- `replace` discards inherited generated labels;
- `off` uses the fixed base host;
- fixed port and base origin remain;
- each atom is exactly one ASCII DNS label;
- reject empty, dot-containing/multi-label, invalid, or oversized labels;
- reject generated labels for IP-literal bases.

## 5.3 Path

```text
path_clause := path atom_list ;
             | path replace atom_list ;
             | path off ;
```

- ordinary form appends local generated segments;
- `replace` discards inherited generated segments;
- `off` retains the fixed base path only;
- each atom is exactly one segment;
- reject empty, `.`, `..`, `/`, and `\\` before percent encoding.

Raw/pre-encoded paths, optional structural segments, route templates, wildcards, and trailing-slash language are unsupported.

# 6. Query patch program

```text
query_clause := query { query_stmt* }
              | query replace { query_stmt* }
              | query off ;

query_stmt   := string_literal = emission ;
              | when_present string_literal = optional_emission ;
              | append string_literal = emission ;
              | remove string_literal ;
```

`optional_emission` uses the same source forms, but the outer source must be Concord-optional.

Query is an ordered multimap patch.

## 6.1 Assignment

```rust
"K" = emission;
```

Remove every current generated occurrence of `K`, then emit zero, one, or many local occurrences at this statement position. Outer optional absence clears.

## 6.2 Conditional override

```rust
when_present "K" = optional_emission;
```

- outer absent: preserve existing entries and positions;
- outer present scalar: replace;
- outer present empty ordered sequence: explicitly clear;
- outer present non-empty sequence: replace with those occurrences.

## 6.3 Append

```rust
append "K" = emission;
```

Preserve existing entries and append emitted occurrences. Optional absence and empty sequences append nothing.

## 6.4 Remove, replace, off

- `remove` removes all generated occurrences of the static key;
- `query replace` starts from an empty generated public query;
- `query off` emits no generated public query;
- neither can remove later private-auth query placement.

Keys are static bounded strings. Dynamic key names and generic query-object serialization are unsupported.

# 7. Public headers

```text
headers_clause := headers { header_stmt* }
                | headers replace { header_stmt* }
                | headers off ;

header_stmt    := string_literal = emission ;
                | when_present string_literal = optional_emission ;
                | append string_literal = emission ;
                | remove string_literal ;
```

Header identity is case-insensitive. Assignment/conditional/append/removal follow the query meanings under header identity.

Explicit repeated occurrences are allowed only for fields where the endpoint author deliberately owns that protocol claim. Known singleton or reserved fields are rejected. Use a scalar adapter for comma/list grammar.

Generic public headers cannot own:

- `Host`;
- transfer/framing/hop-by-hop fields;
- proxy fields;
- `Authorization` or effective private targets;
- `Accept` or transport `Accept-Encoding`;
- request `Content-Type` or `Content-Encoding`;
- cookie fields under the no-cookie model.

No silent precedence repairs collisions.

# 8. Endpoint declaration

```text
endpoint_decl := attrs method ident ( endpoint_parameter_list? )
                 -> RustType [ via RustType ]
                 ( ; | { endpoint_clause* } )

endpoint_clause:= id_clause
                | request_attachment
                | status_clause
                | effect_clause
                | pagination_clause
                | expose_clause
```

Required ordinary parameters and required body parameters become direct arguments in source order. Optional/defaulted values become generated controls.

# 9. HTTP methods

```text
method := GET | HEAD | POST | PUT | PATCH | DELETE | OPTIONS
        | METHOD ( string_literal )
```

Custom method literals are bounded valid HTTP tokens and default conservatively.

Reject `CONNECT` and `TRACE`, including custom spelling. Reject connection upgrades/WebSockets and server-wide `OPTIONS *` because they require a different native connection ownership model.

# 10. Status policy

```text
status_clause := status success ;
               | status status_item ;
               | status [ status_item (, status_item)* ,? ] ;
status_item   := three_digit_status
               | three_digit_status ..= three_digit_status
```

Omission and `status success` mean final `200..=299`.

The macro validates values/ranges, rejects empty sets and confusing overlap, and canonicalizes the finite accepted set for reflection/diff. Source order has no runtime meaning.

Authentication challenge attribution precedes status acceptance. A status cannot be both an effective auth challenge and accepted endpoint outcome.

# 11. Effect class

```text
effect_clause := effect safe ;
               | effect idempotent ;
               | effect non_idempotent ;
```

Defaults:

| Method | Effect |
|---|---|
| GET, HEAD, OPTIONS | `safe` |
| PUT, DELETE | `idempotent` |
| POST, PATCH, custom method | `non_idempotent` |

An explicit declaration may correct provider-specific semantics. An idempotency header does not infer effect.

Policy retry after possible dispatch requires `safe` or `idempotent`. Ordinary pagination requires `safe`.

# 12. Request body

A body slot may be required or optional:

```rust
body input: Input
body input: Input via Representation
body input?: Input
body input?: Option<Input>
```

- required body becomes a direct endpoint argument;
- optional body is initially absent and uses ordinary optional controls;
- outer absence means no native body, no representation invocation, and no body-owned media header;
- a present payload is passed exactly to the representation;
- defaults are forbidden;
- body content is never a public atom or general observation value.

# 13. Response and terminals

The result type is exactly ordinary `.await` output. Complete values, `ByteStream`, and `ItemStream<T>` are distinct caller lifecycles.

Every endpoint supports ordinary await and managed accepted metadata. `accepted_bytes` requires explicit endpoint exposure:

```text
expose_clause := expose accepted_bytes ;
```

It retains auth, negotiation, status, deadline, and finite complete acquisition, while bypassing representation-specific accepted-head/body validation and output conversion.

# 14. Credentials and authentication

```text
credentials_block := credentials { credential_decl* }
credential_decl   := attrs secret ident [ ? ] ;
                   | attrs credential ident = credential_source ;

credential_source := api_key ( secret_ref )
                   | bearer ( secret_ref )
                   | basic ( secret_ref , secret_ref )
                   | custom RustType
                   | endpoint endpoint_path

secret_ref        := secret . ident
endpoint_path     := ident (:: ident)*
```

- required secret is a protected builder input;
- optional secret may be initially absent; a derived slot is unavailable;
- custom provider object is required, though acquisition may report unavailable;
- endpoint-backed source starts empty by source semantics;
- endpoint paths are client-root-relative and may forward-reference a later endpoint;
- the complete graph is resolved and cycles rejected.

Auth attachment:

```text
auth_clause      := auth { auth_placement* }
                  | auth replace { auth_placement* }
                  | auth off ;

auth_placement  := bearer ident challenge_clause? ;
                  | basic ident challenge_clause? ;
                  | authorization = ident challenge_clause? ;
                  | authorization string_literal = ident challenge_clause? ;
                  | header string_literal = ident challenge_clause? ;
                  | query string_literal = ident challenge_clause? ;

challenge_clause := challenge never
                  | challenge unauthorized
                  | challenge unauthorized_or_forbidden
```

- ordinary auth appends placements;
- `replace` discards inherited placements;
- `off` removes all inherited placements;
- at most one effective placement may classify a non-`never` challenge;
- target ownership is exclusive;
- unavailable required material fails before dispatch, never silently omits auth;
- private values appear only in the native request.

# 15. Timeout

```text
timeout_clause := timeout positive_duration ;
```

Every client has one finite positive root timeout. Scope/endpoint omission inherits the nearest value. No `off` form exists. Request-local API may select another validated finite timeout and reset to the retained effective contract timeout.

One absolute deadline covers every core-controlled stage and pagination page. It never restarts for attempts or recovery.

# 16. Rate policies

```text
rate_policies_block := rate_limits { rate_policy_decl* }
rate_policy_decl    := attrs policy ident ( formal_list? ) {
                           id string_literal ;
                           key [ rate_dimension (, rate_dimension)* ,? ] ;
                           cost positive_integer ;
                           rate_window+
                       }
formal_list          := ident (, ident)* ,?
rate_dimension       := origin | endpoint | ident [ via RustType ]
rate_window          := allow positive_integer per positive_duration ;
```

Attachments:

```text
rate_attachment := rate_limit ident ( rate_binding_list? )
                   ( ; | { cost positive_integer ; } )
                 | rate_limit {
                       id string_literal ;
                       key [ bound_rate_dimension (, bound_rate_dimension)* ,? ] ;
                       cost positive_integer ;
                       rate_window+
                   }
```

Rules:

- stable policy ID is required;
- positive cost and at least one positive finite window are required;
- all windows are conjunctive;
- formal bindings are exact and total;
- `origin` is normalized logical scheme/host/effective port, never proxy destination;
- `endpoint` requires the stable endpoint contract ID;
- dimensions require `RateKeyPart` or explicit canonical adapter;
- secrets, credentials, optional values, dynamic dimensions, occurrence expansion, and page-live values are rejected;
- policies compose additively and cannot be disabled by descendants;
- duplicate effective attachment of the same policy identity is rejected.

The DSL declares provider quota semantics. Runtime enforcement is explicitly selected by the application.

# 17. Pagination

```text
pagination_clause := paginate via RustType { pagination_entry+ }
pagination_entry  := ident -> endpoint . ident ;
                   | ident = stable_controller_value ;
stable_controller_value := stable_value | client . ident | scope_ident . ident | endpoint . ident
```

Rules:

- endpoint effect must be `safe`;
- response output must be complete, not lazy stream;
- representation/output pair must be page-capable;
- body must be page-stable when present;
- live destinations are endpoint-owned ordinary scalar slots only;
- no body, secret, auth, timeout, effect, policy, client/scope, repeated, or effective rate-key destination;
- controller owns item extraction and continuation;
- controller description declares bounded response-header interests and whether current page URI is needed;
- no native response or unrestricted header map is exposed.

# 18. Request attachments by owner

| Family | Client | Scope | Endpoint | Inheritance |
|---|---:|---:|---:|---|
| request/response representation default/fence | defaults only | defaults and fences | inline only | nearest owner, independent directions |
| subdomain | yes | yes | no | prepend/replace/off |
| path | yes | yes | yes | append/replace/off |
| query | yes | yes | yes | ordered patch/replace/off |
| public headers | yes | yes | yes | case-insensitive patch/replace/off |
| auth | yes | yes | yes | ordered append/replace/off |
| timeout | yes | yes | yes | nearest finite selection |
| rate attachment | yes | yes | yes | additive only |
| status/effect/pagination/expose | no | no | yes | endpoint-local |

# 19. Diagnostics and generated names

The compiler must validate generated names before emission, including:

- child methods/types/modules;
- `set_`, `with_`, `omit_`, `reset_` controls;
- terminals and planning/description helpers;
- endpoint-backed credential storage methods;
- internal bridge names.

Diagnostics identify the semantic owner, violated rule, conflicting declaration, inherited origin where applicable, and one valid v2.1 correction. Trait assertions should name the endpoint, exact type, and required role rather than exposing expansion internals.

# 20. Unsupported language

The active grammar does not contain:

- generic native request/response mutation;
- arbitrary absolute runtime origin;
- redirects;
- dynamic query/header names;
- generic query-object serialization;
- optional/spread path or subdomain structure;
- raw/pre-encoded paths;
- body defaults;
- generic status callback;
- implicit collection cardinality;
- implicit `Display` protocol encoding;
- descendant rate-policy disable;
- unbounded timeout;
- automatic pagination;
- pagination over lazy stream outputs;
- CONNECT, TRACE, upgrades, WebSockets, cookies, or caching.
