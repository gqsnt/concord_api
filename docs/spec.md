# Concord v3 DSL specification

This document is normative for authored language meaning in Concord v3
(`3.0.0-beta.1`). It restates the implementation specification (sections are
cited as §n); where the two disagree, the implementation specification wins and
this document is a defect. Private compiler representations may differ;
generated and runtime behavior may not. Every `concord` block below is compiled
from `crates/concord-examples/src/docs_spec.rs` or anchored to a compiled
example.

## 1. Lexical conventions

Authored Rust `self::`, `super::`, repeated parent, `crate::`, absolute and
imported paths retain their lexical bindings at the `api!` invocation. The
generator rebases explicit lexical paths in Rust syntax, including nested
generic/qualified types, adapters, representations, constants and pagination
initializers. Exported contract descriptions keep the authored spelling.

A declaration is parsed as Rust token trees with a real Rust syntax parser.
Identifiers and type paths are passed to Rust; the macro never assigns protocol
meaning by recognizing `Option`, `Vec`, an alias or a qualified spelling.

- `RustType` is a syntactically valid Rust type terminated by the surrounding
  DSL delimiter.
- `duration` is a positive integer literal (digit separators allowed) followed
  by `ms`, `s`, `m` or `h`, normalized with checked arithmetic. Fractional,
  negative, zero and overflowing durations and expressions are rejected.
- `stable_value` is a Rust literal, a unary-negative numeric literal, or a Rust
  constant or associated-constant path. Calls, indexing, closures, allocation
  expressions and blocks are rejected; `#[default]` is the explicit route to a
  payload's `Default`.
- String literals may be cooked or raw. Templates apply to the decoded string.
  Structural lengths are UTF-8 bytes.

## 2. Grammar

`?` after a group is optional syntax, `*`/`+` are repetition and a quoted `"?"`
is the literal optional-value marker.

```text
api              := "api!" "{" client "}"
client           := attrs "client" ident "{" client_preamble* child* "}"
child            := scope | endpoint
scope            := attrs "scope" ident "(" parameters? ")"
                    "{" owner_preamble* child* "}"
endpoint         := attrs method ident "(" endpoint_parameters? ")"
                    "->" RustType ("via" RustType)?
                    (";" | "{" endpoint_clause* "}")

client_preamble  := base | values | credentials | rate_registry
                  | owner_attachment
owner_preamble   := owner_attachment
owner_attachment := path | subdomain | query | headers | auth
                  | timeout | request_default | response_default | rate_attachment
endpoint_clause  := path | query | headers | auth | timeout | rate_attachment
                  | endpoint_id | status | effect | absence | pagination | expose

base             := "base" string ";"
values           := "values" "{" (value ";")* "}"
parameters       := value ("," value)* ","?
endpoint_parameters := parameter ("," parameter)* ","?
parameter        := value | body
value            := attrs placement? ident "?"? ":" RustType
                    ("via" RustType)? ("=" stable_value)?
placement        := "query" ("(" string ")")? "each"?
                  | "header" "(" string ")" "each"?
body             := attrs "body" ident "?"? ":" RustType ("via" RustType)?

request_default  := "request" (RustType | "explicit") ";"
response_default := "response" (RustType | "explicit") ";"
timeout          := "timeout" duration ";"
endpoint_id      := "id" string ";"
path             := "path" "replace"? string ";" | "path" "off" ";"
subdomain        := "subdomain" "replace"? atom_list ";"
                  | "subdomain" "off" ";"
atom_list        := "[" (atom ("," atom)* ","?)? "]"
atom             := literal | reference ("via" RustType)?
                  | "concat" "[" atom ("," atom)* ","? "]"
reference        := ident | "client" "." ident | scope_ident "." ident
emission         := atom | "each" reference ("via" RustType)?
query            := "query" "replace"? "{" key_statement* "}"
                  | "query" "off" ";"
headers          := "headers" "replace"? "{" key_statement* "}"
                  | "headers" "off" ";"
key_statement    := string "=" emission ";"
                  | "replace" string "=" emission ";"
                  | "when_present" string "=" emission ";"
                  | "append" string "=" emission ";"
                  | "remove" string ";"

method           := "GET" | "HEAD" | "POST" | "PUT" | "PATCH"
                  | "DELETE" | "OPTIONS" | "METHOD" "(" string ")"
status           := "status" "success" ";"
                  | "status" status_item ";"
                  | "status" "[" status_item ("," status_item)* ","? "]" ";"
status_item      := status_code | status_code "..=" status_code
effect           := "effect" ("safe" | "idempotent" | "non_idempotent") ";"
absence          := "absent" "on" status_code ("," status_code)* ";"
expose           := "expose" "accepted_bytes" ";"

credentials      := "credentials" "{" credential_decl* "}"
credential_decl  := attrs "secret" ident ";"
                  | attrs "credential" ident "=" credential_source ";"
credential_source := "api_key" "(" secret_ref ")"
                   | "bearer" "(" secret_ref ")"
                   | "basic" "(" secret_ref "," secret_ref ")"
                   | "custom" RustType
                   | "endpoint" endpoint_path
secret_ref       := "secret" "." ident
endpoint_path    := ident ("::" ident)*
auth             := "auth" "replace"? "{" placement_decl* "}" | "auth" "off" ";"
placement_decl   := "bearer" ident challenge? ";"
                  | "basic" ident challenge? ";"
                  | "authorization" "=" ident challenge? ";"
                  | "authorization" string "=" ident challenge? ";"
                  | "header" string "=" ident challenge? ";"
                  | "query" string "=" ident challenge? ";"
challenge        := "challenge" ("never" | "unauthorized" | "unauthorized_or_forbidden")

rate_registry    := "rate_limits" "{" (window_set | policy)* "}"
window_set       := attrs "windows" ident "{" window+ "}"
window           := "allow" positive_integer "per" duration ";"
policy           := attrs "policy" ident "(" formals? ")" "{"
                    "id" string ";" "key" "[" formal_dimension_list "]" ";"
                    ("cost" positive_integer ";")? window_source "}"
window_source    := ("using" ident ";") | window+
formals          := ident ("," ident)* ","?
formal_dimension := "origin" | "endpoint"
                  | string "=" ident ("via" RustType)?
bound_dimension  := "origin" | "endpoint"
                  | string "=" reference ("via" RustType)?
rate_attachment  := "rate_limit" ident "(" named_bindings? ")"
                    (";" | "{" "cost" positive_integer ";" "}")
                  | "rate_limit" "{" "id" string ";"
                    "key" "[" bound_dimension_list "]" ";"
                    ("cost" positive_integer ";")? window_source "}"
                  | "rate_limit" string "key" "[" bound_dimension_list "]"
                    ("cost" positive_integer)? "using" ident ";"
named_binding    := ident "=" reference

pagination       := "paginate" "via" RustType "{"
                    "init" RustTypePath "{" init_fields? "}" ";"
                    "cursor" "->" destination ";" "}"
init_fields      := ident ":" init_value ("," ident ":" init_value)* ","?
init_value       := stable_value | reference
destination      := ident | "(" ident ("," ident)+ ","? ")"
```

- A value may carry `via RustType` only with a direct query/header placement;
  that adapter belongs to that placement. An unplaced value selects an adapter
  at each explicit use. A body selects its representation with its own `via`.
- A `literal` atom is a string, character, boolean or numeric literal whose type
  implements the atom role; duration syntax is not an atom. Optional sources
  are permitted only where a keyed emission owns outer presence, never inside
  `concat` or a structural route atom.
- **Multiplicity (LANG-02).** At most one `base`, `values`, `credentials` and
  `rate_limits` per client; one clause per non-rate attachment family per
  owner; one body per endpoint; one `id`, `status`, `effect`, `absent on`,
  `paginate` and `expose` per endpoint. Rate attachments may repeat (§13).
  Duplicated singleton clauses are errors, never last-writer-wins. An empty
  explicit query/header block is legal; an empty `concat` is not.
- Secrets are always required builder inputs. `secret NAME?` is rejected:
  optional static secrets are not supported. Use an endpoint-backed source
  (initially empty) or a custom provider that may report unavailable.

## 3. Ownership

| Family | Client | Scope | Endpoint | Rule |
|---|:-:|:-:|:-:|---|
| Values | Root `values` block | Parameters | Parameters | Owned locally, never implicitly copied into child values |
| Base, credentials, rate registry | Yes | No | No | One closed client definition |
| Request/response default | Yes | Yes | Inline `via` only | Nearest definition, independently by direction |
| Representation fence `explicit` | No | Yes | No | Blocks the inherited default until a new choice |
| Subdomain | Yes | Yes | No | Prepend labels; `replace`/`off` affect generated labels only |
| Path | Yes | Yes | Yes | Append relative segments; `replace`/`off` affect generated segments only |
| Query/header | Yes | Yes | Yes | Ordered checked patch program |
| Auth | Yes | Yes | Yes | Ordered append; explicit `replace`/`off` |
| Timeout | Yes | Yes | Yes | Nearest finite selection; root default 30 seconds |
| Rate attachment | Yes | Yes | Yes | Additive (§13) |
| ID/status/effect/absence/pagination/expose | No | No | Yes | Endpoint-local |

Preambles precede children, so each owner's attachments have one location.
Clause order across different endpoint families does not imply execution
order; within a query/header block statement order is significant. An owner can
have zero child endpoints when it provides a runtime-binding context; every
declared scope remains a visible navigation owner.

## 4. Names and references

- In a client clause, `tenant` binds only the client's `tenant`. In a scope
  preamble, `region` binds only that scope's `region`. In an endpoint clause or
  path template, `id` binds only that endpoint's `id` (LANG-03).
- A missing bare name is an error even when an ancestor has a matching name.
  Diagnostics suggest `client.id` or `regional.id` but never choose.
- A nonlocal reference names an exact visible ancestor: `client.tenant`,
  `regional.language`. A scope identifier is unique along one ancestor chain;
  siblings may repeat names.
- `client`, `endpoint` and the `__concord` prefix are reserved owner names.
  Redundant qualification of the current owner is rejected with a migration
  diagnostic.
- In a named rate policy, bare names are the policy's formals. Credential names
  occupy a separate source namespace; they are not atom values.
- Raw identifiers are supported: `r#type` has logical name `type`. No case
  conversion is performed for an omitted direct query key.

<!-- concord-dsl -->
```rust
scope regions(region: String) {
    GET item(id: String) -> () via Empty {
        path "item-{id}-{regions.region}";
    }
}
```

## 5. Attributes and rustdoc

Supported attributes are documentation (`///`, `#[doc = "..."]`),
`#[deprecated]` / `#[deprecated(note = "...", since = "...")]`, `#[sensitive]`
and `#[default]`. `#[default]` applies only to ordinary values; `#[sensitive]`
only to public ordinary values (bodies and credentials are protected by role).
Unknown attributes and child-level `cfg`/`cfg_attr` are rejected. Documentation
is emitted to the actual caller methods, request types, value controls and
contract descriptions; deprecation is emitted where calling the deprecated
operation triggers the warning. An obsolete optional input can remain as an
explicitly deprecated no-op value; a dead required input cannot.

## 6. Values

### 6.1 State is structural

| Declaration | Effective slot | Initial state | Constructor role |
|---|---|---|---|
| `x: T` | `T` | Caller-supplied | Required |
| `x?: T` | `Option<T>` | Absent | Configurable |
| `x: T = V` | `T` | Retained `V` | Configurable |
| `x?: T = V` | `Option<T>` | Present retained `V` | Configurable |
| `#[default] x: T` | `T` | Retained `T::default()` | Configurable |
| `#[default] x?: T` | `Option<T>` | Present retained `T::default()` | Configurable |

<!-- concord-dsl -->
```rust
client ValueStatesApi {
    base "https://value-states.example.test/";
    values {
        query tenant: String;
        query locale?: String;
        query page_size: u32 = 25;
        query region?: String = "eu";
        #[default] query("tag") each tags: Vec<String>;
        #[default] query("flag") each flags?: Vec<String>;
    }
}
```

`?` adds exactly one Concord-owned outer layer (VALUE-01): a required
`Option<U>` stays required and `x?: Option<U>` has two distinguishable layers.
`Vec<T>` alone is not repeated data; `each` is. Inner optional/null encoding is
an atom or representation responsibility. An inner `None` rendered by an
explicit adapter is domain data, not omission:

<!-- concord-dsl -->
```rust
header("X-Label") label: Option<&'static str> via LabelOrNone = crate::DEFAULT_LABEL;
```

Ordinary values used in shared snapshots must be `Send + Sync + 'static`; bodies
and outputs need `Send + 'static`.

### 6.2 Initialization boundary

Root defaults are evaluated once when the root builder is created; scope
defaults once when the scope handle is selected; endpoint defaults once when the
request or intent is constructed. Binding, cloning, `build` and reset never
evaluate them again. `= stable_value` is type-checked through the closed
`IntoValue<T>` conversions, so `locale: String = "en"` is valid. An optional
initializer initializes `T`, not `Option<T>`. Body initializers and
`#[default]` combined with `= V` are errors.

### 6.3 Controls and transitions

Required uninitialized scope/endpoint parameters (and a required body) are
positional constructor arguments in declaration order. Every root value uses
builder controls; missing required root values accumulate into one
`ClientBuildError`.

| After scope/endpoint construction | `set_x` | `replace_x(Option<T>)` | `set_x_if_some(Option<T>)` | `omit_x` | `reset_x` |
|---|:-:|:-:|:-:|:-:|:-:|
| Required, uninitialized | No | No | No | No | No |
| Initialized required | Payload | No | Yes | No | Yes |
| Uninitialized optional | Payload | Yes | Yes | Yes | No |
| Initialized optional | Payload | Yes | Yes | Yes | Yes |
| Required body | No | No | No | No | No |
| Optional body | Exact payload | Yes | Yes | Yes | No |

On a root builder, required values additionally have `set_x` and
`set_x_if_some`. The transitions are exact (VALUE-03):

```text
set_x(v):                current := Present(convert(v))
replace_x(Some(v)):      current := Present(v)
replace_x(None):         current := Absent
set_x_if_some(Some(v)):  current := Present(v)
set_x_if_some(None):     current := current
omit_x():                current := Absent
reset_x():               current := retained_initial_state
```

Payload setters accept `impl IntoValue<T>`; `replace_x` and `set_x_if_some`
take exact `Option<T>`. For `milestone?: Option<u32>`:

```text
request.set_milestone(None);            // present inner None
request.replace_milestone(None);        // absent outer slot
request.replace_milestone(Some(None));  // present inner None
request.set_milestone_if_some(None);    // no change
```

### 6.4 Closed conversions

`IntoValue<T>` has exactly three library-owned implementations: identity
`T -> T`, `&str -> String` and `&String -> String` (VALUE-04). There is no
numeric widening, `Display` fallback, parsing, array/slice conversion or
recursive optional conversion; domain newtypes are constructed explicitly.

### 6.5 Snapshots and cloning

Built root values are immutable. Clients and scopes are `Clone` by sharing
services and typed snapshots; changing a scope control consumes the handle and
returns a new snapshot. Body-free requests and intents are `Clone` without
requiring `Output: Clone`; each executed clone is a new operation with no retry
authority. Requests whose declaration has a body slot are not `Clone`
(VALUE-05); hold a scope and a request factory instead.

### 6.6 Liveness

Every ordinary input needs a reachable semantic use: a wire emission, a
path/DNS component, a rate dimension, a pagination mapping or an owner's runtime
projection (VALUE-06). A required input used nowhere is an error; an optional
or initialized input used nowhere is an error unless explicitly deprecated as a
no-op bridge. Liveness is computed on resolved symbolic branches.

## 7. Atoms, routes and DNS

### 7.1 Atoms

`RequestAtom::write_atom` is the ordinary scalar role; `AtomEncoder<T>` is the
explicit adapter role selected with `via`. `OrderedValues` supplies stable-order
iteration for `each` over slices, arrays and vectors; unordered collections need
an explicit deterministic adapter. `each` expands one outer-present source into
zero or more occurrences and cannot appear in a path, DNS label, `concat` or
rate dimension. `concat [a, b]` is one scalar atom whose components are total.
A quoted atom in a query/header block is literal data; braces do not
interpolate.

### 7.2 Fixed base

The base is an absolute `http`/`https` URL with a host and no userinfo, query or
fragment (ORIGIN-01). It owns the scheme, host suffix, effective port and base
path; `/root` and `/root/` both prefix generated paths with `/root/`. No request
control or runtime operation can supply another destination, change the
scheme/port or discard the base path. Redirects, cookies and upgrades are
disabled; a proxy changes connection routing only.

### 7.3 Relative path templates

<!-- concord-dsl -->
```rust
client RouteApi {
    base "https://route.example.test/root";
    values {
        api_version: String = "v2";
    }
    subdomain ["api"];
    path "{api_version}";

    scope regional(region: String) {
        subdomain [region];
        path "regions/{region}";

        GET item(id: String) -> () via Empty {
            path "items/{id}";
        }
        GET details(id: String) -> () via Empty {
            path "resource-{id}/details";
        }
        GET report(date: Day) -> () via Empty {
            path "reports/{date via IsoDate}";
        }
        GET legacy() -> () via Empty {
            path replace "legacy/{client.api_version}";
        }
        GET origin_root() -> () via Empty {
            path off;
        }
        GET braces() -> () via Empty {
            path "literal-{{braces}}";
        }
    }

    scope global() {
        subdomain replace ["global"];
        GET status() -> () via Empty;
    }

    scope apex() {
        subdomain off;
        GET ping() -> () via Empty;
    }
}
```

- No leading slash, trailing slash or empty segment (PATH-01). A literal `/`
  separates segments only outside a hole.
- A hole is `{reference}` or `{reference via RustType}` naming a declared total
  value. It never declares an input, evaluates an expression or expands several
  segments. `{{` and `}}` are literal braces.
- A segment mixing literals and holes is one bounded concatenated atom; it must
  not be empty, `.` or `..`, and contain no `/` or backslash. Each segment is
  percent-encoded exactly once; a literal `?` or `#` is rejected.
- `path "x"` appends; `path replace "x"` drops inherited generated segments,
  then appends; `path off` drops them and appends nothing (PATH-02). The
  normalized base prefix always stays.

### 7.4 DNS labels

`subdomain [...]` prepends labels to inherited generated labels;
`subdomain replace [...]` and `subdomain off` remove generated labels, never the
base suffix (ORIGIN-02). Each atom is one lowercase-normalized ASCII DNS label
of 1-63 bytes (letters, digits, interior hyphens). Dotted, empty and non-ASCII
values are rejected, the complete host must fit DNS limits, and generated labels
on an IP-literal base are rejected. An endpoint cannot own a subdomain clause.

## 8. Query/header composition

### 8.1 One symbolic ordered program

Contributions are composed symbolically before any encoder runs (PATCH-01).
At each owner, each family is normalized in this exact order:

1. Start with the inherited program, or clear it if the owner selects the
   family `replace` form.
2. Add the family's direct value placements in declaration order.
3. Apply the family's explicit block statements in source order.

`query off` with a direct query placement (or `headers off` with a direct
header placement) is an error; use a replacement block. A `query replace { ... }`
may coexist with direct placements: it clears inheritance before both. Potential
ownership is independent of runtime presence.

### 8.2 Operations

| Operation | Precondition | Present scalar/sequence | Outer absent | Present-empty `each` |
|---|---|---|---|---|
| `"k" = e` | No prior potential `k` | Introduce occurrences | Emit none | Emit none |
| `replace "k" = e` | Prior potential `k` | Remove previous; emit here | Remove previous | Remove previous |
| `when_present "k" = e` | Prior potential `k`; outer-optional source | Remove previous; emit here | Keep prior occurrences and positions | Remove previous |
| `append "k" = e` | None | Append occurrences | No change | No change |
| `remove "k"` | Prior potential `k` | Remove all earlier | Same | Same |

Introduction conflicts are errors for every source shape; the diagnostic names
both writers (PATCH-02). `replace`, `when_present` and `remove` require an
earlier owner, which catches misspelled override targets. Query keys are
case-sensitive, nonempty and at most 256 bytes; header names compare
case-insensitively, are valid field-name tokens and at most 256 bytes. Keys are
never interpolated.

### 8.3 Selection, evaluation and order

Outer presence and surviving occurrences are decided over the complete program
before any `RequestAtom`, adapter or item encoder runs (PATCH-03). Each
surviving scalar is encoded once per preparation; each item of a surviving
`each` source once; an absent source is never iterated. A suppressed ancestor
adapter that would fail cannot fail the request. Presence reads the selected
outer slot, never the adapter.

With parent entries `a=1, locale=fr, z=9`:

```text
when_present locale := None       => a=1, locale=fr, z=9
when_present locale := Some(de)   => a=1, z=9, locale=de
replace locale := None            => a=1, z=9
append locale := Some(de)         => a=1, locale=fr, z=9, locale=de
```

A replacement is placed at the replacing operation, not at the old position.
The compiled program behind this table:

<!-- concord-dsl -->
```rust
client PatchApi {
    base "https://patch.example.test/";
    values {
        locale: String = "fr";
    }
    query {
        "a" = "1";
        "locale" = locale;
        "z" = "9";
    }

    GET conditional(override_locale?: String) -> () via Empty {
        query { when_present "locale" = override_locale; }
    }
    GET exact(override_locale?: String) -> () via Empty {
        query { replace "locale" = override_locale; }
    }
    GET appended(extra_locale?: String) -> () via Empty {
        query { append "locale" = extra_locale; }
    }
    GET removed() -> () via Empty {
        query { remove "locale"; }
    }
    GET fresh(query page?: u32) -> () via Empty {
        query replace { "only" = "this"; }
    }
    GET bare() -> () via Empty {
        query off;
    }
}
```

### 8.4 One value, several uses

Direct placement is a shorthand for a normal introduction, not a prohibition on
explicit mappings (LANG-01). Setting `locale` sets all three emissions; omitting
it omits all three:

<!-- concord-dsl -->
```rust
GET inspect(query locale?: String) -> Item {
    path "inspect";
    headers {
        "Accept-Language" = locale;
    }
    query {
        "language" = locale;
    }
}
```

Explicit blocks reference local and ancestor values alike:

<!-- concord-dsl -->
```rust
scope regions(region: String) {
    subdomain replace [region, "api"];

    GET item(id: String, tags: Vec<String>) -> () via Empty {
        path "v1/items/{id}";
        query {
            "locale" = client.locale;
            "tag" = each tags;
        }
        headers {
            "X-Region" = regions.region;
        }
    }
}
```

### 8.5 Sensitivity

`#[sensitive]` classifies a public value (PRIVACY-01). The value is sent, but
suppressed from plans, error displays, retry classifiers, observers and debug
output. A query/header key's sensitivity is sticky along the root-to-owner
chain, including a sensitive contribution later removed, replaced or fenced; a
sensitive component makes a concatenated atom, a path segment or a generated
host sensitive. Mandatory name-based redaction additionally matches
`authorization`, `proxy-authorization`, `cookie`, `set-cookie`, `token`,
`access-token`, `refresh-token`, `secret`, `password`, `passwd`, `credential`,
`session`, `api-key`, `apikey` and `client-secret` (case-insensitive, `_` read as
`-`).

### 8.6 Header ownership

Public, private-auth, body-media, negotiation and transport headers have
distinct owners; potential collisions are rejected before presence is known
(HEAD-01). `Host`, `Authorization`, `Proxy-Authorization`, `Proxy-Authenticate`,
`Accept`, `Accept-Encoding`, `Content-Type`, `Content-Encoding`,
`Content-Length`, `Transfer-Encoding`, `Connection`, `Keep-Alive`, `TE`,
`Trailer`, `Upgrade`, `Proxy-Connection`, `Cookie` and `Set-Cookie` are never
public targets, nor is a target claimed by an effective auth placement. The
singleton fields `User-Agent`, `Referer`, `From`, `Date`, `If-Modified-Since`,
`If-Unmodified-Since`, `If-Range`, `Range`, `Max-Forwards`, `Origin`,
`Access-Control-Request-Method` and `Access-Control-Request-Headers` cannot
repeat. Values are validated; CR/LF/NUL injection is rejected.

## 9. Representation defaults

Request and response defaults are independent; the nearest default wins for its
direction. A scope's `request explicit;` or `response explicit;` removes the
inherited default for descendants until another default or an inline `via`
supplies one. An inline body/result `via` wins only in its direction. A
bodyless endpoint needs no request representation; every result needs an
effective response representation. Selection is by Rust trait obligations, not
result-type spelling (REP-01).

<!-- concord-source: crates/concord/tests/generated/representation_inheritance.rs#representation_inheritance_contract -->
<!-- concord-dsl -->
```rust
api! {
    client RepresentationInheritanceApi {
        base "https://inheritance.example.test/";
        timeout 30s;
        request Text;
        response ParentBuffered;

        POST inherited(body input: String) -> String;

        scope overrides() {
            request Text;
            response Text;

            POST local(body input: String) -> String;
        }

        scope fenced() {
            request explicit;
            response explicit;

            POST explicit(body input: String via Text) -> String via Text;
        }
    }
}
```

The fence is provenance: removing it changes future parent-edit behavior even
when today's bytes are equal. See [Request bodies](request_bodies.md) and
[Responses](responses.md).

## 10. Methods, status, effect and absence

<!-- concord-dsl -->
```rust
client OutcomeApi {
    base "https://outcome.example.test/";
    response Json;

    GET find(id: String) -> Option<Item> {
        path "items/{id}";
        absent on 404, 410;
    }
    GET strict(id: String) -> Item {
        path "strict/{id}";
        expose accepted_bytes;
    }
    POST create(body input: UpdateInput via Json) -> Item {
        path "items";
        status 201;
    }
    PUT upsert(id: String, body input: UpdateInput via Json) -> Item {
        path "items/{id}";
        status [200..=201, 204];
        effect idempotent;
    }
    DELETE delete(id: String) -> () via Discard {
        path "items/{id}";
        status success;
    }
    METHOD("PROPFIND") properties(id: String) -> String via Text {
        path "items/{id}";
        effect safe;
    }
}
```

- **Status.** The default accepted set is 200-299 (`status success`). A single
  code, list or inclusive range replaces it. Codes are 100-599; sets are
  nonempty, increasing and disjoint; a final accepted or absence status below
  200 is rejected, and no accepted status may overlap an effective challenge.
- **Effect.** Defaults are `safe` for `GET`/`HEAD`/`OPTIONS`, `idempotent` for
  `PUT`/`DELETE`, `non_idempotent` for `POST`/`PATCH`/`METHOD(...)`. A custom
  method is a bounded HTTP token; `CONNECT`, `TRACE` and `OPTIONS *` are
  rejected.
- **Expected absence.** `absent on 404;` is a separate endpoint-owned status set
  (OUTCOME-01). The result must be `Option<T>` (checked through the sealed
  `OptionalOutcome` relationship, so an alias works) and the present
  representation must use the buffered driver. On a declared absence status the
  body is dropped unread and the result is `None`. It cannot combine with
  pagination, a streaming/discard driver, `store_as_*` or
  `expose accepted_bytes`. A plain `-> Option<T>` without `absent on` has no
  status shortcut.
- **Accepted bytes.** `expose accepted_bytes;` adds the `.accepted_bytes()`
  terminal for buffered or discard normal pairs (§10.4).

## 11. Credentials and authentication

Sources are not placements. `credentials` names where material comes from;
`auth` blocks place it (§11.1):

<!-- concord-dsl -->
```rust
client AuthenticationApi {
    base "https://authentication.example.test/";
    credentials {
        secret token;
        secret user;
        secret password;

        credential api_token = api_key(secret.token);
        credential bearer_token = bearer(secret.token);
        credential login = basic(secret.user, secret.password);
    }

    GET bearer_auth() -> () via Empty {
        auth { bearer bearer_token challenge unauthorized; }
    }
    GET basic_auth() -> () via Empty {
        auth { basic login challenge unauthorized_or_forbidden; }
    }
    GET exact_auth() -> () via Empty {
        auth { authorization = bearer_token; }
    }
    GET scheme_auth() -> () via Empty {
        auth { authorization "Token" = bearer_token challenge unauthorized; }
    }
    GET header_auth() -> () via Empty {
        auth { header "X-API-Key" = api_token; }
    }
    GET query_auth() -> () via Empty {
        auth { query "token" = api_token; }
    }
}
```

- Each `secret` is a required builder input named after the secret; a `custom`
  provider is a required builder input named after the credential.
- `endpoint path` sources start empty and name a source endpoint by
  root-relative path (forward references allowed); cycles and missing paths are
  rejected. The source endpoint gets a `store_as_<credential>()` terminal.
- `auth` appends ordered placements, `auth replace` replaces inherited ones and
  `auth off` removes them (AUTH-01). At most 64 placements, exclusive targets,
  at most one non-`never` challenge; an omitted challenge is `never`.
- Missing required material fails before dispatch; a placement is never
  silently omitted. See [Authentication](auth.md).

## 12. Timeouts

The nearest finite `timeout` wins; an omitted root timeout selects 30 seconds.
There is no infinite timeout.

<!-- concord-dsl -->
```rust
client TimeoutApi {
    base "https://timeout.example.test/";
    timeout 30s;

    GET milliseconds() -> () via Empty { timeout 250ms; }
    GET seconds() -> () via Empty { timeout 5s; }
    GET minutes() -> () via Empty { timeout 10m; }
    GET hours() -> () via Empty { timeout 1h; }
    GET no_timeout() -> () via Empty { timeout 30s; }
}
```

## 13. Rate policies

Numbers and state use different declarations (§12.1). A window set holds only
positive capacities and durations; it grants no identity and shares no state. A
policy has an explicit ID and an ordered key schema.

<!-- concord-dsl -->
```rust
rate_limits {
    windows standard { allow 20000 per 10s; allow 1200000 per 10m; }
    policy tenant_reads(tenant) {
        id "catalog.tenant.read";
        key [origin, "tenant" = tenant];
        allow 120 per 1m;
        allow 5000 per 1h;
    }
    policy shared_reads() {
        id "catalog.shared.read";
        key [origin];
        using standard;
    }
}
rate_limit "concord.riot.21" key [origin] using standard;
```

Named and inline attachments bind values explicitly:

<!-- concord-dsl -->
```rust
scope accounts(account: String, region: String) {
    rate_limit account_reads(account = account) { cost 2; }

    GET item(item_id: String) -> () via Empty {
        id "rate_limit_api.accounts.item";
        rate_limit {
            id "service.item.read";
            key [
                origin,
                endpoint,
                "client.tenant" = client.tenant,
                "accounts.region" = accounts.region,
                "endpoint.item_id" = item_id,
            ];
            cost 2;
            allow 100 per 1m;
        }
    }
}
```

- Named bindings are exact and total. A policy uses one window set or inline
  windows, not both. Cost defaults to 1. Repeated window durations, zero or
  overflowing values and a cost above any capacity are rejected.
- Attachments are lexical and additive (RATE-01); nothing disables an ancestor
  constraint. An exact structural repetition is deduplicated; a conflicting
  repetition of an ID is an error.
- Every key has 1-32 dimensions with unique labels. `origin` and `endpoint` are
  reserved built-ins; custom labels are explicit strings of at most 128 bytes,
  bound to required, non-page-live ordinary values. `"client.tenant"` and
  `"tenant"` are different schemas even with equal values. A `via` names a
  `RateKeyEncoder`.
- `endpoint` requires the endpoint to declare an explicit `id`.

See [Rate limits](rate_limits.md).

## 14. Endpoint identity

An explicit `id` pins operational identity; otherwise it derives
`snake_case(ClientName).scope_path.endpoint_name` (IDENTITY-01). The derived ID
changes on relevant renames or moves; it is navigation-derived, not stable
across refactors. IDs are ASCII, 1-128 bytes, dot-separated segments of at most
63 bytes, with lowercase alphanumeric segment ends; derived and explicit
collisions are rejected, and an invalid derivation requires an explicit ID
(never truncated or hashed).

<!-- concord-dsl -->
```rust
GET by_id(id: String) -> Item via Json {
    id "catalog.item.by-id";
    path "items/{id}";
}
```

## 15. Pagination declarations

`init RustTypePath { ... }` constructs the controller's public `Init` value;
`cursor -> slot` maps the controller's entire `Cursor` value onto the endpoint's
entire slot (PAGE-01), or a tuple cursor onto several slots with exactly
corresponding arity and types. Destinations are unique endpoint-owned scalar
slots, never bodies, ancestor values, credentials, `each` sources or values in
an effective rate key. Pagination requires `effect safe`, a buffered response
driver and, when a body is declared, a `PageRequestRepresentation`.

<!-- concord-dsl -->
```rust
GET page(
    query offset: u64,
    query page_size: std::num::NonZeroU32,
) -> Vec<u32> via Json {
    paginate via concord::pagination::OffsetPagination {
        init concord::pagination::OffsetInit { offset: offset, page_size: page_size };
        cursor -> offset;
    }
}
```

The declaration adds the traversal terminals `.pages(n)`,
`.collect_prefix(n)` and `.collect_all(NonZeroU32)`; ordinary `.await` still
returns one page. See [Pagination](pagination.md).

## 16. Diagnostics

Every custom diagnostic has a stable code, the owning endpoint/scope, a primary
span, related spans and a correction valid in v3 (DIAG-01).

| Code | Trigger |
|---|---|
| `C3-NAME` | Unbound, ambiguous or redundant local qualification |
| `C3-PLACE` | Unused required input, invalid direct placement/adapter, duplicate same-key introductions |
| `C3-OVERRIDE` | Introduced inherited key or missing override target (both sites shown) |
| `C3-ATOM` | Scalar/each/adapter trait mismatch |
| `C3-PATH` | Bad hole, segment, braces or slash in a template |
| `C3-OWNER` | Unsupported owner clause or forbidden authority |
| `C3-REP` | Missing direction default or fence, invalid pair, absence capability conflict |
| `C3-AUTH` | Missing or cyclic source, conflicting target, multiple challenge owners, accepted/challenge overlap |
| `C3-RATE` | Missing/invalid ID, label/codec conflict, duplicate effective policy, optional/live key input |
| `C3-PAGE` | Init/Cursor mismatch, forbidden destination, incomplete-driver page capability |
| `C3-GENERATED` | Generated Rust symbol collision (both origins named, never renamed) |
| `C3-MIGRATE` | A recognized retired v2 form, with its precise v3 spelling |

## 17. Unsupported language

V3 has no optional path/DNS structure, host templates, raw or pre-encoded path
APIs, wildcard or trailing-slash routes, arbitrary URL templates, dynamic key
expressions, body defaults, infinite timeouts, a status-callback DSL or implicit
`Option`/`Vec` wire inference. Applications that need a different fixed prefix
declare a different client.

## 18. V3-only language boundary

The parser accepts canonical V3 and rejects the following retired forms with
`C3-MIGRATE` or `C3-OWNER` diagnostics:

- Retired v2 `vars { ... }`: write `values`.
- Retired v2 `host [...]`: write `subdomain`.
- Retired v2 `path ["items", id]`: write the relative template `path "items/{id}"`.
- Retired v2 `endpoint.x` and redundant current-owner qualification: write the bare local name.
- Retired v2 `using Representation`: write `via`.
- Retired v2 `= default`: write `#[default]`.
- Retired v2 `retry` clauses: select a `RetryPolicy` in runtime configuration.
- Retired v2 `fmt[...]`: write `concat [...]`.
- Retired v2 `+=`: write `append`.
- Retired v2 `#[redact]`: write `#[sensitive]`.
- Retired v2 typed secrets and unlabelled rate dimensions are rejected.

The [version policy](version_policy.md) selects V3 alone. No automatic converter
or compatibility runtime is provided.
