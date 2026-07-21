# CONCORD_DSL_SPEC_0.1

**Version:** 0.1  
**Status:** Normative Release Final
**Authority:** Concord DSL language specification  
**Requirement prefix:** `DSL`  
**Depends on:** Specification Index, Product Contract  
**Supersedes:** None

## 1. Scope

This document defines the exact language accepted by the `api!` procedural macro for Concord 0.1.

It owns:

- lexical forms;
- grammar;
- declaration ordering;
- namespaces and references;
- composition and inheritance;
- feature-dependent syntax;
- compile-time validation;
- diagnostic requirements.

Generated Rust names and method signatures are defined by `CONCORD_GENERATED_API_SPEC_0.1`.

## 2. Lexical elements

### 2.1 Identifiers

An identifier uses Rust identifier syntax, including raw identifiers where Rust permits them.

Examples:

```text
Api
users
r#type
```

Identifiers are case-sensitive.

### 2.2 Rust type paths

A Rust type path uses a `syn::TypePath`-compatible path without generic expressions outside normal Rust type arguments.

Examples:

```text
String
crate::dto::User
crate::codec::MessagePack<CreateUser>
```

### 2.3 Rust item paths

Extension function and type references use a `syn::Path`-compatible path.

Examples:

```text
crate::retry::api_policy
crate::pagination::VendorCursor
crate::rate_limit::ProviderObserver
```

### 2.4 String literals

String literals use Rust string-literal syntax.

### 2.5 Integer literals

Positive integer fields use unsuffixed base-10 Rust integer literals.

### 2.6 Duration literals

A duration literal is:

```text
INTEGER UNIT
```

without whitespace between the integer and unit.

Supported units:

```text
ms
s
m
h
```

Examples:

```text
250ms
5s
10m
1h
```

**DSL-LEX-003 — Duration representability.** Duration values MUST be representable as `std::time::Duration`.

### 2.7 Comments

Rust line and block comments are accepted wherever token separation permits them.

### 2.8 Semicolons

Configuration statements and entries require a trailing semicolon unless their grammar ends with a nested braced block.

**DSL-LEX-001 — Case sensitivity.** User identifiers and names MUST be resolved case-sensitively.

**DSL-LEX-002 — Duration validity.** Invalid, zero-forbidden, or overflowing duration values MUST produce compile-time diagnostics.

## 3. Top-level grammar

Informative EBNF notation:

```text
api_declaration  = "api" "!" "{" client_declaration+ "}" ;

client_declaration =
    "client" identifier "{" client_body "}" ;
```

The actual macro invocation is:

```rust
api! {
    client Api {
        // ...
    }
}
```

At least one client declaration is required.

**DSL-CLIENT-001 — Unique client names.** Client names MUST be unique within one macro invocation.

**DSL-TOP-001 — Multiple clients.** One `api!` invocation MAY contain more than one client declaration.

**DSL-TOP-002 — Client uniqueness.** Duplicate client names in one invocation MUST be rejected.

## 4. Client body ordering

Informative grammar:

```text
client_body =
    base_clause
    client_var*
    credentials_block?
    retry_clause?
    host_clause?
    path_layer?
    query_layer?
    headers_layer?
    auth_clause*
    timeout_clause?
    rate_limits_block?
    rate_limit_clause*
    client_child* ;

client_child = scope_declaration | endpoint_declaration ;
```

Once the first scope or endpoint begins, no client configuration declaration may follow.

Exactly one `base` declaration is required.

**DSL-ORDER-001 — Client ordering.** Client configuration MUST precede every child scope and endpoint.

**DSL-ORDER-002 — Base uniqueness.** A client MUST contain exactly one `base` declaration.

## 5. Base URL

Syntax:

```text
base "https://api.example.com";
```

**DSL-BASE-003 — Absolute safe base URL.** The literal MUST parse as an absolute HTTP or HTTPS URL, contain a host, and contain no user information, query, or fragment.

Required components:

- a scheme of `http` or `https`;
- a host.

Forbidden components:

- user information;
- query;
- fragment.

A fixed port and fixed path are permitted.

**DSL-BASE-001 — Static base.** The 0.1 DSL base URL MUST be a string literal, not a dynamic expression.

**DSL-BASE-002 — Base validation.** Invalid base URLs MUST be rejected during generated-client compilation or construction as specified by the Generated API and Execution specifications.

## 6. Value declarations

### 6.1 Client values

Required:

```text
var tenant: String;
```

Optional:

```text
var locale: Option<String>;
```

Defaulted:

```text
var page_size: u32 = 100;
```

Informative grammar:

```text
client_var =
    "var" identifier ":" rust_type
    ("=" default_expr)? ";" ;
```

Optionality is expressed only through the Rust type `Option<T>`.

The DSL does not provide a `?` optionality suffix.

### 6.2 Default expressions

Defaults use a restricted, side-effect-resistant grammar rather than arbitrary Rust expressions.

```text
default_expr =
      string_literal
    | signed_integer_literal
    | boolean_literal
    | rust_const_path ;
```

A `rust_const_path` is a Rust path resolved by the downstream compiler and is intended for constants, unit enum variants, and `None`. Function calls, method calls, blocks, macros, closures, `await`, mutation, struct literals, array construction, and arbitrary operators are not accepted as defaults in 0.1.

A default expression cannot reference DSL values, secrets, credentials, request-local values, or generated internal identifiers.

Evaluation rules:

- a client default is evaluated during `build()` only when the default remains selected;
- a builder override prevents evaluation of the unselected client default;
- a scope default is evaluated once when its facade snapshot is created;
- an endpoint default is evaluated once when its pending request snapshot is created;
- scope and endpoint `reset_*` operations restore their retained resolved value without re-evaluation.

Rust type checking remains responsible for proving that a selected default expression has the declared value type.

**DSL-VAL-002 — No dependent defaults.** A DSL default expression MUST NOT depend on another DSL-declared value or credential.

**DSL-VAL-003 — Selected default evaluation.** An unselected client default MUST NOT be evaluated; a selected default MUST be evaluated once for its owning snapshot, and scope/endpoint reset behavior MUST reuse the retained resolved value.

**DSL-VAL-004 — Restricted defaults.** Default expressions MUST use only the side-effect-resistant forms defined in this section.

### 6.3 Value names

**DSL-VAL-005 — Unique client value names.** Client value names MUST be unique within the client.

**DSL-VAL-001 — Rust optionality.** Optional values MUST use `Option<T>`.

## 7. Restricted public expressions

Request-shaping expressions use this grammar:

```text
public_expr =
      string_literal
    | integer_literal
    | "vars" "." identifier
    | "scope" "." identifier
    | "ep" "." identifier
    | format_expr ;

format_expr = "fmt" "[" public_expr ("," public_expr)* "]" ;
```

`fmt[...]` concatenates each atom's `Display` representation without a separator.

Examples:

```text
vars.tenant
ep.user_id
fmt["v", ep.version]
```

The following are not accepted as public expressions:

- arbitrary Rust blocks;
- method calls;
- function calls;
- field access beyond the defined namespaces;
- indexing;
- arithmetic;
- closures;
- secret references;
- credential references.

**DSL-EXPR-001 — Restricted grammar.** Request-shaping expressions MUST use only the forms defined in this section.

**DSL-EXPR-002 — No secrets.** Secrets and credentials MUST NOT be expressible through the public expression grammar.

**DSL-EXPR-003 — Format commas.** `fmt[...]` atoms MUST be comma-separated.

## 8. Credentials

### 8.1 Block

A client may contain at most one credentials block:

```text
credentials {
    secret api_key: String;
    credential application_key = api_key(secret.api_key);
}
```

Informative grammar:

```text
credentials_block =
    "credentials" "{" credential_entry* "}" ;

credential_entry = secret_entry | credential_entry_def ;
```

### 8.2 Secrets

```text
secret NAME: String;
```

Concord 0.1 supports required `String` secrets only.

Optional secrets and secret defaults are not supported.

**DSL-SECRET-001 — Unique secret names.** Secret names MUST be unique in the credentials block.

### 8.3 API-key credential

```text
credential NAME = api_key(secret.SECRET_NAME);
```

### 8.4 Bearer credential

```text
credential NAME = bearer(secret.SECRET_NAME);
```

### 8.5 Basic credential

```text
credential NAME = basic(
    secret.USERNAME_SECRET,
    secret.PASSWORD_SECRET
);
```

### 8.6 OAuth2 client credentials

```text
credential NAME = oauth2_client {
    token_url: "https://auth.example.com/oauth/token",
    client_id: secret.CLIENT_ID_SECRET,
    client_secret: secret.CLIENT_SECRET_SECRET,
    scope: "users.read",
};
```

Fields:

- `token_url`: required HTTPS string literal with a host and no user information or fragment;
- `client_id`: required secret reference;
- `client_secret`: required secret reference;
- `scope`: optional string literal.

Unknown or duplicate fields are rejected.

This source requires the `json` feature.

The built-in provider performs an isolated provider HTTP request with these semantics:

```text
POST token_url
Authorization: Basic base64(form_encode(client_id):form_encode(client_secret))
Content-Type: application/x-www-form-urlencoded

grant_type=client_credentials
&scope=...                 // only when declared
```

The token endpoint response must be a successful JSON object containing:

```text
access_token: non-empty string       // required
token_type: string                   // optional; when present, must equal "bearer" case-insensitively
expires_in: non-negative integer     // optional seconds, representable as u64
```

Additional response fields are ignored. A `refresh_token` field does not enable refresh-token grant behavior in Concord 0.1.

The Basic username and password components and form fields use OAuth-compatible `application/x-www-form-urlencoded` encoding.

When `expires_in` is present, Concord computes expiration with checked time arithmetic. An unrepresentable duration is an authentication-provider error. When absent, the token has no provider-declared expiration.

Refresh obtains another token through the same client-credentials request. Provider request/response limits and provider isolation rules apply. A non-success token response or invalid token object becomes a safe authentication-provider error; the response body is never copied into the application error.

### 8.7 Endpoint-backed credential

```text
credential NAME = endpoint scope::Endpoint;
```

The referenced endpoint must exist in the same client.

Its decoded output type must satisfy the generated credential-material requirements.

The endpoint must not effectively authenticate with the same credential it produces.

### 8.8 Custom provider credential

```text
credential NAME = custom crate::auth::SessionToken;
```

The Rust type path names the credential material type.

The generated client builder requires an application-provided credential provider for this credential.

### 8.9 Credential names

**DSL-CRED-005 — Unique credential names.** Credential names MUST be unique in one credentials block.

A credential declaration may reference only secrets declared earlier in the same block.

**DSL-CRED-001 — One credentials block.** A client MUST contain at most one credentials block.

**DSL-CRED-002 — Secret confinement.** Secret references MUST occur only inside built-in credential-source declarations.

**DSL-CRED-003 — Definition order.** A built-in credential source MUST reference secrets already declared earlier in the block.

**DSL-CRED-004 — Endpoint cycle rejection.** Statically discoverable endpoint-backed credential dependency cycles MUST be rejected.

## 9. Retry

At most one retry declaration is permitted per client.

Disabled:

```text
retry off;
```

Explicit Reqwest default behavior:

```text
retry reqwest_default;
```

Dangerous custom:

```text
retry custom crate::retry::api_policy;
```

Absence means retry disabled.

`retry custom` requires `dangerous-custom-retry`.

Retry declarations are client-only.

**DSL-RETRY-001 — Client-only retry.** Retry MUST NOT be declared on scopes or endpoints.

**DSL-RETRY-002 — Disabled by absence.** An omitted retry declaration MUST select disabled application native retry.

**DSL-RETRY-003 — Explicit Reqwest opt-in.** Reqwest default retry behavior MUST be selected only through `retry reqwest_default;` or the equivalent public HTTP builder method.

## 10. Host composition

Syntax:

```text
host [scope.region, "api"];
```

Clearing inherited prefix labels:

```text
host off;
```

A host declaration replaces the complete inherited host-prefix list. Resolved labels are prefixed to the base host in declaration order: `host ["eu", "api"]` with base host `example.com` produces `eu.api.example.com`.

Each expression produces exactly one ASCII DNS label.

**DSL-HOST-005 — Valid resolved DNS label.** Every resolved ordinary host label MUST satisfy all of the following:

- contain between 1 and 63 ASCII bytes;
- contain only ASCII letters, ASCII digits, or `-`;
- begin and end with an ASCII letter or digit.

The final host produced by prefixing the base host must remain a valid URL host. Unicode labels, raw internationalized host names, underscores, ports, and multi-label values are not accepted through ordinary host composition. Punycode labels are accepted when they satisfy the ASCII label rule.

At most one host declaration or `host off` is permitted per layer.

**DSL-HOST-001 — Replace semantics.** A local `host [...]` declaration replaces, rather than appends to, inherited host-prefix labels.

**DSL-HOST-002 — One label per atom.** Every host expression MUST resolve to exactly one validated DNS label.

**DSL-HOST-003 — ASCII DNS form.** Ordinary dynamic host labels MUST satisfy the ASCII letter/digit/hyphen rule and the 63-byte per-label bound.

**DSL-HOST-004 — Prefix order.** Host labels MUST be prefixed to the base host from left to right in declaration order.

## 11. Path composition

Append:

```text
path ["v1", "users", ep.user_id];
```

Replace:

```text
path replace ["health"];
```

Clear:

```text
path off;
```

Every atom produces exactly one non-empty path segment.

**DSL-PATH-006 — Static segment boundary.** Static path atoms MUST NOT contain `/` or `\`.

A resolved static or dynamic segment equal to `.` or `..` is rejected. Dynamic values are encoded exactly once as path segments; percent characters in input are encoded as data rather than interpreted as pre-encoded delimiters.

At most one path declaration is permitted per layer.

Path composition operates after the fixed path embedded in the base URL. When DSL segments are present, one separator joins them to the non-empty base path regardless of whether the base literal ended in `/`; no duplicate separator is introduced. With no DSL segments, the parsed base path is preserved. `path replace` and `path off` affect only DSL-composed path segments; they never remove the base URL's fixed path.

**DSL-PATH-001 — Append by default.** Ordinary path declarations append local segments to inherited path segments.

**DSL-PATH-002 — Segment integrity.** No path atom may create more than one path segment.

**DSL-PATH-003 — No traversal.** Dynamic path values MUST NOT alter path structure through slash, backslash, dot-segment, or pre-encoded delimiter injection.

**DSL-PATH-004 — Non-empty non-dot segments.** Every resolved DSL path atom MUST produce one non-empty segment other than `.` or `..`.

**DSL-PATH-005 — Base-path joining.** DSL path segments MUST join the parsed fixed base path with exactly one separator and MUST NOT remove or duplicate the base-path boundary.

## 12. Query composition

Modify inherited query:

```text
query {
    "locale" = vars.locale;
    "tag" = ep.tags;
    remove "debug";
}
```

Replace inherited query:

```text
query replace {
    "locale" = vars.locale;
}
```

Clear:

```text
query off;
```

Informative grammar:

```text
query_layer =
      "query" query_mode? "{" query_entry* "}"
    | "query" "off" ";" ;

query_mode = "replace" ;

query_entry =
      string_literal "=" public_expr ";"
    | "remove" string_literal ";" ;
```

**DSL-QUERY-004 — Unique local query keys.** A query block MUST NOT mention the same key more than once, including removal.

Value semantics are determined by the resolved Rust type:

- scalar `T`: one pair;
- `Option<T>`: zero or one pair;
- `Vec<T>`: one pair per element in vector order;
- `Option<Vec<T>>`: absent or vector expansion.

Each emitted value uses `Display` before URL query encoding.

Ordinary query blocks modify inherited query by key:

- assignment removes inherited values for that key and appends the new emitted pairs;
- removal removes all inherited values for that key;
- unrelated inherited pairs retain their relative order;
- local emitted pairs follow the surviving inherited pairs in local declaration order.

**DSL-QUERY-001 — Deterministic order.** Public query pair order MUST be deterministic and preserve vector element order.

**DSL-QUERY-002 — Duplicate key rejection.** One query block MUST NOT assign or remove the same key more than once.

**DSL-QUERY-003 — Auth exclusion.** Concord-managed query authentication is not a public query entry and is not part of query composition.

## 13. Header composition

Modify inherited headers:

```text
headers {
    "X-Tenant" = vars.tenant;
    remove "X-Deprecated";
}
```

Replace inherited headers:

```text
headers replace {
    "X-Tenant" = vars.tenant;
}
```

Clear:

```text
headers off;
```

Header values may resolve from scalar `T` or `Option<T>`.

Vectors are not supported for public header declarations in 0.1.

Header names are compared case-insensitively.

**DSL-HEAD-004 — Unique local header names.** One header block MUST NOT mention the same name more than once case-insensitively.

An assignment replaces all inherited values for the same name.

**DSL-HEAD-001 — Valid HTTP values.** Resolved header values MUST be valid HTTP header values.

**DSL-HEAD-002 — Duplicate rejection.** Duplicate header names in one block MUST be rejected case-insensitively.

**DSL-HEAD-003 — Reserved ownership.** Public headers MUST NOT claim names reserved by transport, authentication, cookies, or proxy authentication. `Content-Type` is additionally reserved when the selected body source declares or intrinsically owns a content type; otherwise an explicit public `Content-Type` is permitted.

## 14. Operation deadline

Set an inherited Concord operation deadline:

```text
timeout 5s;
```

Disable the inherited operation deadline:

```text
timeout off;
```

Absence inherits the nearest declaration.

At most one timeout declaration is permitted per layer.

The deadline applies to one endpoint or page operation and includes asynchronous hook, credential, provider HTTP, limiter wait, native HTTP, authentication-recovery, and buffered response work as defined by the Execution Specification. It is distinct from connection and read timeouts configured on the managed HTTP clients.

**DSL-TIME-001 — Three-state deadline model.** Timeout composition distinguishes inheritance, explicit disablement, and an explicit Concord operation deadline.

**DSL-TIME-002 — Page-local deadline.** A paginated endpoint's resolved timeout applies independently to each page operation rather than to the entire pagination collection.

## 15. Rate-limit declarations

### 15.1 Direct lexical declarations

The normal generated DSL has one optional complete declaration at each lexical owner:

```text
rate_limit {
    cost 2;
    100 / 1m;
}
```

Informative grammar:

```text
rate_limit_decl =
    "rate_limit" "{"
        cost_clause?
        rate_window+
    "}" ;

rate_limit_observer_decl = "rate_limit_observer" rust_path ";" ;
cost_clause = "cost" positive_integer ";" ;
rate_window = positive_integer "/" duration_literal ";" ;
```

Client, scope, and endpoint may each contain at most one direct declaration. Cost defaults to one;
windows and cost are positive, and cost must not exceed any capacity. Declaration order is static
and deterministic. A client rule applies to every endpoint, a scope rule applies to its complete
lexical subtree, and an endpoint rule applies only to that endpoint.

**DSL-RATE-008 — Complete lexical rule declaration.** A direct declaration MUST contain an optional
positive static cost and at least one positive window; cost MUST NOT exceed any window capacity.

**DSL-RATE-001 — Positive windows.** Window capacity and duration MUST be positive in every direct
lexical declaration.

**DSL-RATE-002 — Valid cost.** Cost MUST be positive and MUST NOT exceed any window capacity.

### 15.2 Independent observer

The optional observer is a separate, unique, client-only declaration:

```text
rate_limit_observer crate::rate_limit::ProviderObserver;
```

The selected type must implement `Default + RateLimitObserver`. It configures only the generated
default governor and is valid whether or not the client itself owns a direct quota.

**DSL-RATE-012 — Independent client rate-limit observer.** An observer declaration MUST be optional,
unique, and client-only, and MUST be independent of quota declaration and explicit limiter selection.

### 15.3 Lexical composition and removed syntax

Effective generated rules compose automatically in this exact order: client, outer scope to inner
scope, endpoint. Child rules add to ancestors and cannot replace, disable, or modify an ancestor.
There is no normal generated support for discontiguous quota groups; a desired group without a
coherent lexical owner belongs to a custom limiter or advanced prepared endpoint.

**DSL-RATE-009 — Lexical subtree composition.** The effective plan for every endpoint MUST be the
ordered ancestor-to-descendant sequence of direct declarations.

**DSL-RATE-010 — Ancestor preservation and no local opt-out.** A child MUST NOT replace, disable, or
modify an ancestor declaration.

**DSL-RATE-011 — No templates or generated key dimensions.** The normal DSL MUST reject
`rate_limits { ... }`, named templates, named attachments, `by [...]`, `rate_limit NAME`,
`rate_limit replace NAME`, `rate_limit off`, `observe PATH;`, host/method/endpoint/`vars`/`scope`/
`ep` dimensions, arbitrary expressions, formatted key parts, secrets, and credentials. Normal
generated rules use declaration-scoped global buckets; request values do not partition them.

The migration forms are `rate_limit { ... }` for a direct declaration and
`rate_limit_observer PATH;` for an observer. The old forms are intentionally rejected with focused
diagnostics rather than adapted.

## 17. Authentication placement

Bearer:

```text
auth bearer session;
```

Basic:

```text
auth basic login;
```

Header:

```text
auth header "X-Api-Key" = application_key;
```

Query:

```text
auth query "api_key" = application_key;
```

Challenge suffix:

```text
challenge unauthorized
challenge unauthorized_or_forbidden
challenge never
```

Examples:

```text
auth header "X-Api-Key" = application_key challenge never;
auth bearer session challenge unauthorized;
```

Challenge defaults are source-aware:

| Credential source | Omitted challenge suffix |
|---|---|
| Built-in OAuth2 client-credentials bearer | `unauthorized` |
| Static API key | `never` |
| Static bearer | `never` |
| Static Basic | `never` |
| Endpoint-backed credential | `never` |
| Custom provider | invalid; an explicit suffix is required |

Replacement:

```text
auth replace bearer administrative_session;
```

Clearing:

```text
auth off;
```

Ordinary auth declarations append to inherited placements.

At most one replacement is permitted per layer and it must precede other local auth declarations.

`auth off` is exclusive within the layer.

Credential and placement compatibility is validated at compile time where statically known.

An effective authentication plan may contain at most one placement whose resolved challenge policy is not `never`. This avoids attributing one ambiguous `401` or `403` response to several unrelated credentials.

**DSL-AUTH-001 — Placement ownership.** Bearer and Basic placements MUST own `Authorization`.

**DSL-AUTH-002 — Collision rejection.** Effective auth placements MUST NOT collide with one another or with materially present public header/query entries.

**DSL-AUTH-003 — Source-aware challenge default.** An omitted challenge suffix MUST resolve to `unauthorized` only for the built-in OAuth2 client-credentials source and MUST resolve to `never` for built-in static and endpoint-backed sources.

**DSL-AUTH-004 — Off exclusivity.** `auth off` MUST NOT appear with another auth declaration in the same layer.

**DSL-AUTH-005 — Custom challenge explicitness.** An auth placement referencing a custom credential provider MUST declare an explicit challenge policy.

**DSL-AUTH-006 — Single automatic challenger.** One effective authentication plan MUST contain at most one placement whose resolved challenge policy is not `never`.

## 18. Scopes

Syntax:

```text
scope users(
    region: String,
    locale: Option<String>,
    page_size: u32 = 100,
) {
    host [scope.region];
    path ["v1", "users"];

    // child scopes and endpoints
}
```

Informative grammar:

```text
scope_declaration =
    "scope" identifier
    "(" parameter_list? ")"
    "{" scope_body "}" ;
```

Scope parameters use the same required, optional, and defaulted type forms as endpoint parameters.

**DSL-SCOPE-003 — Unique scope parameter names.** Scope parameter names MUST be unique in the scope.

A scope body contains configuration followed by child scopes and endpoints.

The client configuration order applies analogously, excluding client-only declarations.

Scopes may contain:

- `host`;
- `path`;
- `query`;
- `headers`;
- `auth`;
- `timeout`;
- `rate_limit`;
- child scopes;
- endpoints.

Scopes may not contain:

- `base`;
- client `var` declarations;
- `credentials`;
- `retry`;
- `rate_limits` templates.

**DSL-SCOPE-001 — Child ordering.** Scope configuration MUST precede child scopes and endpoints.

**DSL-SCOPE-002 — Depth bound.** Scope nesting MUST respect the maximum defined by `BOUND-COMPILE-001`.

## 19. Endpoints

### 19.1 Standard methods

```text
GET ById(user_id: String) -> Json<User> {
    path [ep.user_id];
}
```

Supported keywords:

```text
GET POST PUT PATCH DELETE HEAD OPTIONS TRACE CONNECT
```

### 19.2 Custom method

```text
METHOD("PROPFIND") Inspect() -> Text {
    path ["inspect"];
}
```

**DSL-END-005 — Valid custom method token.** The custom method literal MUST be a valid HTTP method token.

### 19.3 Alias

```text
GET ById(user_id: String) as by_id -> Json<User> {
    path [ep.user_id];
}
```

The alias controls the generated facade method name.

### 19.4 Parentheses

Endpoint parentheses are mandatory, including for zero parameters:

```text
GET Health() -> NoContent;
```

A bodyless endpoint with no clauses may end with a semicolon.

An endpoint with clauses uses a block.

### 19.5 Parameters

Required:

```text
user_id: String
```

Optional:

```text
verbose: Option<bool>
```

Defaulted:

```text
limit: u32 = 100
```

Body specification parameter:

```text
body: Json<CreateUser>
```

An endpoint may contain at most one body parameter.

**DSL-PARAM-001 — Body parameter position.** The body parameter MUST be last in the parameter list.

**DSL-PARAM-002 — Unique endpoint parameter names.** Endpoint parameter names MUST be unique.

### 19.6 Clause order

Inside an endpoint block:

1. `host`;
2. `path`;
3. `query`;
4. `headers`;
5. `auth`;
6. `timeout`;
7. `rate_limit`;
8. `paginate`.

Repeated auth and rate-limit clauses retain declaration order.

`paginate` must be last.

**DSL-END-001 — Typed response header.** One arbitrary Rust response specification type MUST appear in the endpoint header after `->` and MUST remain a `syn::Type` rather than a macro semantic category.

**DSL-END-002 — Parentheses required.** Every endpoint declaration MUST include parentheses.

**DSL-END-003 — Body last.** A body parameter MUST be the final endpoint parameter.

**DSL-END-004 — Clause order.** Endpoint clauses MUST follow the order defined in this section.

## 20. Request body specification types

The final parameter named `body` selects one arbitrary Rust type. That type is checked by Rust as a
`RequestBodySpec`, and the generated method argument is its associated `Input`. The macro retains
the type without determining whether it is a codec, byte body, stream, multipart body, managed
body, built-in, or application extension.

### 20.1 JSON

```text
body: Json<CreateUser>
```

Requires `json`.

### 20.2 Text

```text
body: Text
```

Generated input is `String`.

### 20.3 Custom buffered codec or request specification

```text
body: crate::codec::MessagePack<CreateUser>
```

An `EncodeBody` type automatically supplies the buffered request specification. Any other type may
implement the public `RequestBodySpec` contract directly.

### 20.4 Bytes

```text
body: Bytes
```

Generated input is `bytes::Bytes`.

### 20.5 Stream

```text
body: Stream<OctetStream>
```

The media marker must implement the public media contract.

### 20.6 Multipart

```text
body: Multipart
```

Requires `multipart`.

Concord 0.1 multipart is constructed through `MultipartBody`; the DSL does not define a typed multipart form model.

### 20.7 Managed request body

```text
body: RequestBody
```

Generated input is the one public managed outgoing `RequestBody` value.

**DSL-BODY-001 — No-content request marker.** `NoContent` MUST NOT satisfy the request-body specification contract; a bodyless request MUST omit the body parameter and expose no public request-specification parameter.

**DSL-BODY-002 — Multipart request only.** `Multipart` MUST be available only as a request specification under the `multipart` feature and MUST NOT become a response specification or multipart response parser.

**DSL-BODY-003 — Arbitrary request specification type.** The final parameter named `body` MUST retain one arbitrary `syn::Type`, generated code MUST require that type to implement `RequestBodySpec`, and the macro MUST NOT classify it by name, crate origin, input type, codec, media policy, managed-body form, or semantic category.

## 21. Response specification types

The type following `->` is one arbitrary Rust type checked by Rust as a `ResponseBodySpec`. The
macro retains the type without selecting a response mode, codec, media rule, or built-in category.

### 21.1 JSON

```text
-> Json<User>
```

Requires `json`.

### 21.2 Text

```text
-> Text
```

Direct output is `String`.

### 21.3 Custom buffered codec or response specification

```text
-> crate::codec::MessagePack<User>
```

`DecodeBody` types automatically supply buffered response specifications. Any other type may
implement the public `ResponseBodySpec` contract directly and select one core-sealed acquisition
mode.

### 21.4 Bytes

```text
-> Bytes
```

### 21.5 No content

```text
-> NoContent
```

### 21.6 Stream

```text
-> Stream<OctetStream>
```

### 21.7 Records

```text
-> Records<crate::records::Decoder>
```

Requires the specification type and decoder to be available under the selected core records
features. `Records<Csv<T>>` and `Records<NdJson<T>>` are the built-in typed record forms.

### 21.8 Unsupported response specifications

`Multipart` does not implement `ResponseBodySpec`.

**DSL-RESP-001 — Response compatibility.** Pagination MUST be declared only when the selected response specification implements `PageResponseSpec` and its output implements `IntoIterator`; body/response compatibility and direct/metadata output typing MUST otherwise be determined by public specification traits rather than macro semantic categories.

**DSL-RESP-003 — Arbitrary response specification type.** The type following `->` MUST be retained as one arbitrary `syn::Type`, generated code MUST require that type to implement `ResponseBodySpec`, and the macro MUST NOT classify it by name, crate origin, output type, codec, media policy, acquisition mode, or semantic category.

**DSL-RESP-004 — Capability-gated pagination response.** A pagination declaration MUST require the selected response type to implement `PageResponseSpec` and its output to implement `IntoIterator`; `Bytes`, `NoContent`, `Stream<M>`, and `Records<D>` MUST NOT acquire pagination through macro name matching or an accidental blanket capability.

## 22. Pagination declaration

### 22.1 Uniform controller struct

Every pagination declaration names one arbitrary Rust type and constructs that controller through a mandatory struct-shaped field block. Built-in and application controllers use the same form:

```text
paginate concord_core::advanced::OffsetPagination {
    offset: bind ep.offset,
    page_size: bind ep.limit,
}

paginate concord_core::advanced::PagePagination {
    page: bind ep.page,
    page_size: bind ep.per_page,
}

paginate concord_core::advanced::CursorPagination<String> {
    cursor: bind ep.cursor,
}

paginate crate::pagination::VendorCursor {
    cursor: bind ep.cursor,
    region: scope.region,
    empty_pages: 0,
    max_empty_pages: vars.max_empty_pages,
}
```

Each named field is one member of the complete Rust struct literal generated for the controller. Ordinary Rust checking diagnoses missing, unknown, inaccessible, or type-incompatible fields and types that do not implement the controller contract.

### 22.2 Live bindings and stable initializers

`bind ep.FIELD` and `bind scope.FIELD` are the only writable binding forms. Initial construction reads the destination value into the controller field once. Before each page, generated code clones the original endpoint snapshot and copies the current controller field into that request-local endpoint or scope field. The per-page clone is never copied back into the controller.

An unbound initializer is evaluated once during controller construction and remains controller-local. Accepted stable initializers are direct `vars.FIELD`, `scope.FIELD`, or `ep.FIELD` references; string, integer, and boolean literals; and the simple Rust paths or constants accepted by the restricted public-expression grammar. Calls, method calls, closures, async blocks, arbitrary blocks, formatting expressions, secrets, credentials, request bodies, and transport/native values are outside this grammar.

Only direct request-local endpoint and scope snapshot fields are writable. Client configuration, shared client state, variables, secrets, credentials, bodies, and transport/native state are not binding destinations.

### 22.3 Grammar

Informative grammar:

```text
pagination_clause =
    "paginate" rust_type "{" pagination_field_list? "}" ;

pagination_field_list =
    pagination_field ("," pagination_field)* ","? ;

pagination_field =
    identifier ":" pagination_initializer ;

pagination_initializer =
      "bind" ("ep" | "scope") "." identifier
    | restricted_public_expr ;
```

Field shorthand, struct rest syntax, indirect binding expressions, and a missing block are rejected. The block may end in a trailing comma. Pagination remains permitted only on bodyless endpoints, and the pagination clause remains the final endpoint clause.

The macro validates declaration structure, unique controller fields, unique resolved binding destinations, restricted initializer syntax, reference ownership, and writable-destination ownership. Rust type checking owns controller field visibility, completeness, names, types, and the controller trait bound.

**DSL-PAGE-001 — Bodyless pagination.** An endpoint with a request body MUST NOT declare pagination.

**DSL-PAGE-002 — Unique pagination fields and bindings.** Duplicate controller field names or duplicate resolved binding destinations MUST be rejected.

**DSL-PAGE-003 — Last clause.** Pagination MUST be the final endpoint clause.

**DSL-PAGE-007 — Complete controller struct construction.** A pagination declaration MUST name one Rust type and a mandatory field block that generates one complete ordinary Rust struct literal; every declared field initializer MUST be evaluated exactly once during initial controller construction.

**DSL-PAGE-008 — Inline live binding round trip.** A `bind ep.FIELD` or `bind scope.FIELD` initializer MUST copy that request-local destination into the identically named controller field exactly once during construction and MUST copy the current controller field into the same destination of each fresh per-page endpoint clone.

**DSL-PAGE-009 — Writable binding boundary.** `bind` MUST accept only a direct `ep.FIELD` or `scope.FIELD` destination and MUST NOT target variables, secrets, credentials, bodies, transport/native values, or shared client configuration.

**DSL-PAGE-010 — Restricted controller initializer grammar.** An unbound controller initializer MUST be limited to a direct `vars.FIELD`, `scope.FIELD`, or `ep.FIELD` reference, a string, integer, or boolean literal, or an approved simple Rust path or constant; shorthand, rest syntax, calls, method calls, closures, async blocks, arbitrary blocks, formatting expressions, and protected references MUST be rejected.

## 23. Name resolution

The macro maintains separate namespaces for:

- clients;
- client values;
- secrets;
- credentials;
- scopes within each parent;
- endpoints within each parent;
- rate-limit templates.

Endpoint and scope child names must be unique within one parent after generated Rust-name normalization.

Aliases must be unique within the target facade.

References resolve lexically through the current client and scope chain.

`scope.NAME` resolves a visible scope parameter. If multiple nested scopes expose the same name ambiguously, the reference is rejected.

**DSL-NAME-001 — No ambiguous references.** A public value reference that cannot be resolved uniquely MUST be rejected.

**DSL-NAME-002 — Generated collision rejection.** Distinct DSL declarations that normalize to the same generated public Rust name in one namespace MUST be rejected unless an explicit alias resolves the collision.

## 24. Composition normalization

Before code generation, every scope and endpoint receives a fully normalized configuration.

### 24.1 Host

- absence: inherit complete prefix;
- `host [...]`: replace complete prefix;
- `host off`: clear prefix.

### 24.2 Path

- absence: inherit;
- ordinary `path`: append;
- `path replace`: replace;
- `path off`: clear.

### 24.3 Query and headers

- absence: inherit;
- ordinary block: modify by key/name;
- `replace`: clear inherited values, then apply block;
- `off`: clear.

### 24.4 Authentication and rate limits

- absence: inherit;
- ordinary statements: append;
- one leading replacement: clear inherited, then add;
- `off`: clear and prohibit other same-layer statements.

### 24.5 Timeout

- absence: inherit;
- duration: set;
- `off`: disable.

**DSL-NORM-001 — Fully resolved IR.** Code generation MUST receive normalized configuration without unresolved inheritance operations.

## 25. Feature validation

**DSL-FEAT-001 — JSON gate.** The `Json<T>` request/response specification and built-in OAuth2 client credentials MUST require the core `json` feature through type and trait availability rather than macro category selection.

**DSL-FEAT-002 — Multipart gate.** The `Multipart` request specification and multipart request APIs MUST require the core `multipart` feature through type and trait availability and MUST add no multipart response behavior.

**DSL-FEAT-003 — Custom retry gate.** `retry custom` requires `dangerous-custom-retry`.

Feature failures must identify the required feature and the declaration requiring it.

**DSL-FEAT-004 — Type-owned body and response feature gates.** Request/response feature gating MUST be owned by `concord_core` type and trait availability; the macro MUST NOT recognize built-in or application specification names to select, reject, or silently substitute body/response behavior, and a disabled required feature MUST fail compilation.

## 26. Compile-time bounds

The exact values are defined by the Security and Limits Specification.

The parser must bound:

- nested scopes;
- nested `fmt[...]` expressions.

Exceeding a bound must produce a controlled diagnostic rather than stack overflow or unbounded recursion.

## 27. Diagnostic requirements

**DSL-DIAG-001 — Focused span.** A diagnostic MUST identify the smallest useful source span.

**DSL-DIAG-002 — Primary cause.** A diagnostic SHOULD report one primary cause and MAY add related notes.

**DSL-DIAG-003 — Canonical correction.** For invalid supported syntax, diagnostics SHOULD show the canonical form.

**DSL-DIAG-004 — No secrets.** Diagnostics MUST NOT evaluate or reproduce secret values.

**DSL-DIAG-005 — No generated internals.** DSL diagnostics MUST NOT require users to understand private generated traits or modules.

**DSL-DIAG-006 — Deterministic output.** Equivalent invalid input MUST produce deterministic diagnostic wording and spans, subject to compiler span formatting.

## 28. Canonical complete example

```rust
api! {
    client ExampleApi {
        base "https://example.com/api";

        var region: String;
        var tenant: String;
        var locale: Option<String>;

        credentials {
            secret api_key: String;
            secret client_id: String;
            secret client_secret: String;

            credential application_key =
                api_key(secret.api_key);

            credential oauth = oauth2_client {
                token_url: "https://auth.example.com/oauth/token",
                client_id: secret.client_id,
                client_secret: secret.client_secret,
                scope: "users.read",
            };

            credential session = endpoint auth::Login;
            credential vendor = custom crate::auth::VendorToken;
        }

        retry off;

        headers {
            "X-Tenant" = vars.tenant;
        }

        timeout 5s;

        auth header "X-Api-Key" = application_key;

        rate_limits {
            application {
                by [host];
                500 / 10s;
                30000 / 10m;
            }

            tenant {
                by [host, endpoint, vars.tenant];
                cost 2;
                100 / 1m;
            }

            observe crate::rate_limit::ProviderObserver;
        }

        rate_limit application;

        scope auth() {
            POST Login(
                body: Json<LoginRequest>,
            ) -> Json<AccessToken> {
                path ["auth", "login"];
                auth off;
            }
        }

        scope users(region: String) {
            host [scope.region];
            path ["v1", "users"];
            rate_limit tenant;

            GET ById(
                user_id: String,
                verbose: Option<bool>,
            ) as by_id -> Json<User> {
                path [ep.user_id];
                query {
                    "verbose" = ep.verbose;
                    "locale" = vars.locale;
                }
                auth bearer oauth;
            }

            GET List(
                offset: u64 = 0,
                limit: std::num::NonZeroU32,
            ) -> Json<Vec<User>> {
                query {
                    "offset" = ep.offset;
                    "limit" = ep.limit;
                }
                auth bearer oauth;
                paginate concord_core::advanced::OffsetPagination {
                    offset: bind ep.offset,
                    page_size: bind ep.limit,
                }
            }

            POST Create(
                body: Json<CreateUser>,
            ) -> Json<User> {
                auth bearer session;
            }

            GET Download(user_id: String)
                -> Stream<OctetStream>
            {
                path [ep.user_id, "archive"];
            }

            POST Upload(
                body: Multipart,
            ) -> NoContent {
                path ["upload"];
            }
        }
    }
}
```
