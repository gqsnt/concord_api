# Concord V4 semantics

Normative description of the implemented language and runtime. Rationale is in `DECISIONS.md`;
executable evidence is listed in `TEST_MATRIX.md`.

## 1. Declaration

```text
api!       := doc* vis "client" Name params? "{" "base" STRING ";" stmt* child* "}"
child      := cattr* "scope" name params? "{" stmt* child* "}"
            | cattr* METHOD name params ("->" payload)? (";" | "{" stmt* "}")
cattr      := doc | "#[cfg(..)]" | "#[deprecated..]"
METHOD     := GET | POST | PUT | PATCH | DELETE | HEAD | OPTIONS
params     := "(" (param ("," param)* ","?)? ")"
param      := attr* placement? name "?"? ":" Type ("=" Expr)?
            | doc* "body" name "?"? ":" payload                  (endpoints only, at most one)
payload    := Type "via" Type                (payload type, representation)
placement  := "query" ("(" STRING ")")? "each"? | "header" "(" STRING ")" "each"?
attr       := doc | "#[sensitive]"
stmt       := "path" STRING ";" | "subdomain" STRING ";"
            | "query" "{" entry* "}" | "headers" "{" entry* "}"
entry      := op? STRING "=" rhs ("else" "inherit")? ";" | "remove" STRING ";"
op         := "append" | "override"
rhs        := STRING                      (template)
            | "each"? reference
reference  := name | owner "." name       (owner = `client` or an enclosing scope's name)
```

- One `api!` declares exactly one client. Each owner (client, scope, endpoint) declares each
  statement at most once, and statements precede child declarations.
- Attributes: documentation everywhere; `#[sensitive]` on values; on scopes and endpoints also
  `#[cfg(..)]` (applied to every generated item of that owner and its descendants, and to the
  parent's navigation method) and `#[deprecated..]` (applied to the navigation method, so the
  warning appears where callers navigate). Other attributes are rejected. Generated names are
  still checked as if every `cfg` were enabled: two cfg-exclusive endpoints cannot share a name.
- Templates (`path`, `subdomain`, quoted query/header values) contain literal text and holes
  `{name}` / `{owner.name}`; `{{` and `}}` are literal braces (with or without holes).
- No meaning is derived from Rust type spellings. `Option<T>` and `Vec<T>` are ordinary types.

## 2. Names

- A bare name refers to a value of the **current owner only**. A missing bare name is an error even
  if an ancestor declares it; the diagnostic suggests the qualified form.
- `client.x` and `<scope>.x` refer to a value of that enclosing owner. Ancestors are searched before
  the "this is the current owner" diagnostic, so an endpoint may share a name with an enclosing
  scope. Scope names are unique along a chain; `client` is reserved.
- Hole names cannot be `self`, `Self`, `super`, `crate` or `_` (raw or not).
- Every declared value must be used by a placement, template or reference; an unused value is an
  error. A value is used either as a scalar or with `each`, not both.
- Generated names must not collide: per generated type, navigation methods, value controls
  (`set_x`, `maybe_x`) and built-ins (`new`, `with_base_url`, `send`, `prepare`, `clone`) are
  checked and a collision is a compile error naming both origins. Value and body names may not
  start with `__`; a client value may not be named `http`.

## 3. Values

| Declaration | Stored | Initial | Supplied / changed by |
|---|---|---|---|
| `x: T` | `T` | — | client: field of `{Client}Config`; scope/endpoint: positional argument of the entry call; then `set_x(v)` |
| `x: T = E` | `T` | `E` | `set_x(v)` |
| `x?: T` | `Option<T>` | absent | `set_x(v)` (present), `maybe_x(o)` (exactly `o`) |
| `x?: T = E` | `Option<T>` | present `E` | `set_x(v)`, `maybe_x(o)` |
| `body b: T via R` | `T` | — | positional argument; then `set_b(v)` |
| `body b?: T via R` | `Option<T>` | absent | `set_b(v)`, `maybe_b(o)` |

- Entry calls: `Client::new(http: reqwest::Client, config: {Client}Config)` when the client has
  required values (`Client::new(http)` otherwise); `parent.scope(...)`, `parent.endpoint(...)`.
  `{Client}Config` is a generated struct with one public field per required client value, in
  declaration order; its `Debug` redacts like the client's. Scope and endpoint positional
  arguments are the required values and a required body, **in declaration order**.
- Positional arguments, `set_*` and initializers accept `impl IntoValue<T>` (config fields are
  exactly `T`): exactly `T`, or
  `&str`/`&String` for `String`. Nothing else converts (no `Into`, no numeric widening, no
  wrapping into `Option`). `maybe_*` takes exactly `Option<T>`, so that `None` needs no type
  annotation (`maybe_x(Some("a"))` for a `String` value is a type error; write
  `set_x("a")`).
- **Initializers.** `E` is any Rust expression, evaluated exactly once each time its owner is
  constructed (`new`, each scope entry, each endpoint call), in declaration order, even when it is
  immediately overridden. Cloning, setters, `prepare` and `send` never evaluate it. `E` is compiled
  inside a nested `fn` item: it can use items of the invoking module but cannot read other values,
  the entry arguments or `self`.
- **Snapshots.** Client and scope handles are immutable `Arc` snapshots; setters consume the handle
  and return a new one (copy-on-write). A child captures a clone of its parent at entry, so later
  changes to the parent produce new handles and never alter existing children or requests.
  Endpoint requests are plain structs. The snapshot is **structural**: it fixes which value each
  handle holds, not the inside of a value — an `Arc<Mutex<T>>`, an atomic or a custom `Display`
  is shared, and values are formatted when a request is prepared. A setter on a shared client or
  scope handle copies that owner's values once (copy-on-write); setters on a unique handle, scope
  entry, endpoint calls, `prepare` and `send` copy none.
- No `reset`: to return to an initializer, use the original handle or a fresh entry call.
- `x?: Option<U>` has three states: absent (`maybe_x(None)`), present `None` (`set_x(None)`),
  present `Some(u)`.
- Client and scope value types must be `Clone` (handles are copy-on-write snapshots). An endpoint
  request is `Clone` exactly when all of its own value types and its body value are `Clone`
  (`Clone` on the parent handle is always available); otherwise it is simply not `Clone`, and a
  move-only value is accepted. Concord never clones a value or body to prepare or send. `Clone`
  says nothing about whether re-sending is safe: replay policy belongs to the application.

## 4. URL composition

**Base.** Declared with `base`; replaceable at runtime with `with_base_url(url)`. It must be an
`http`/`https` URL with a host, optional port and path, and no user information, query or fragment.
The declared base must additionally fall within a strict subset (ASCII DNS name, dotted IPv4 or
bracketed IPv6 host; decimal port 1–65535; `pchar` path without `%`) checked at compile time, so the
generated constructor cannot fail. Port 0 is rejected in both forms, and a declared base may not contain `xn--`
(punycode) labels, which URL parsing validates further (use `with_base_url`, which returns an
error instead). The base path is the canonical path produced by URL parsing. When generated
segments follow, at most one trailing `/` is removed before joining (`/root/` + `a` →
`/root/a`, `/root//` + `a` → `/root//a`); without generated segments the base path is used
exactly as given. It is never resolved relative to anything.

**Path.** Each owner's `path` template is split on literal `/` into segments; segments of all owners
are appended root first after the base path. Templates are relative: no leading `/`, no empty
segment, no literal `.`/`..` segment; literal text must be RFC 3986 `pchar` without `%` and is sent
verbatim. A trailing `/` is allowed only on an endpoint's own `path` and produces a trailing slash.
Holes are formatted with `Display` and percent-encoded with every byte except unreserved
(`A-Z a-z 0-9 - . _ ~`) escaped, so a value is always exactly one segment (`a/b` → `a%2Fb`,
`%2F` → `%252F`). A segment that is empty, `.` or `..` after formatting is a build error. A hole may
not refer to an optional value. If no owner has a path, the base path is used unchanged.

**Subdomain.** Each owner's `subdomain` template is split on literal `.` into labels. Labels are
prepended to the base host, the deepest owner's labels leftmost (`scope region { subdomain
"{region}" }` under `subdomain "api"` on base `example.com` → `eu.api.example.com`). Each label,
after formatting and ASCII lowercasing, must be 1–63 ASCII letters, digits or interior hyphens;
the host must not exceed 253 bytes; labels require a domain-name base, not an IP address. Literal
labels are checked at compile time; values at build time. IDNA is not performed.

**Query.** Pairs are `application/x-www-form-urlencoded` (space → `+`), exactly as reqwest's
`.query()`. The `?` is written only if at least one pair survives.

## 5. Query and header composition

Query parameters and headers are two independent *families*. For one endpoint, contributions are
collected along the chain root → scopes → endpoint; within one owner, direct placements (in
parameter order) come before the owner's block statements (in source order). Keys compare
case-sensitively for query and case-insensitively for headers (header names are emitted lowercase).

Each key has one **slot**, positioned at the key's first appearance on the chain. The operations:

| Statement | Requires a live writer earlier on the chain | Effect on the key's slot |
|---|---|---|
| `"k" = v`, direct placement | must **not** exist | becomes live with `v` |
| `append "k" = v` | — | `v` is added after the existing occurrences; slot becomes live |
| `override "k" = v` (`v` always present) | must exist | the slot holds exactly `v` |
| `override "k" = v else inherit` (`v` can be absent) | must exist | if `v` is present: the slot holds exactly `v`; if absent: unchanged |
| `remove "k"` | must exist | emptied; no longer live, so a later `"k" = v` is legal |

- An `override` whose source reads an optional value must end with `else inherit`, and only such
  an override may: the declaration states what absence means. (An error names both choices.)
- Exact replacement ("absent means absent") is written `remove "k"; "k" = v` — `v` may be a
  direct placement of the same owner (`GET g(query k?: String) { query { remove "k"; } }`).
  Within one owner, `remove` statements act first, on what the owner inherits; the owner's direct
  placements and other statements follow.
- A removed and later reintroduced key keeps the slot position of its first appearance.
- Liveness and slot positions are decided by the declaration, at compile time, never by which
  optional values are present at runtime: an introduction that reads an absent optional value
  sends nothing, yet the key is live (so a descendant may `override` it, not introduce it) and
  keeps its slot position. (Confirmed by the independent composition oracle,
  `tests/oracle_composition.rs`.)
- A source is **present** when every optional value it reads is present. A present `each` source
  with zero items is present and emits nothing — so it still replaces under `override`.
- `each` iterates any `IntoIterator` (`&T: IntoIterator`, items `Display`). On a required
  `Option<T>` that means zero or one occurrence, so `override "k" = each opt` with `None` clears
  `k` — chosen explicitly by writing `each`; Concord does not inspect the type. Declared as
  `opt?: Option<T>` with `override "k" = each opt else inherit`, the caller gets all three
  choices: inherit (`maybe_opt(None)`, the default), omit (`set_opt(None)`), replace
  (`set_opt(Some(v))`).
- **Order on the wire.** Query: slots in first-appearance order, and within a slot the occurrences
  in contribution order — the query string is exactly that sequence. Headers: the values of one
  name are sent in contribution order. The order *between different header names* is not part of
  the contract: the request carries an `http::HeaderMap`, which groups values by name and does not
  promise an order across names, and reqwest/hyper add their own headers. Interleaved repeats
  across query keys (`f=a&op=eq&f=b`) cannot be declared; edit the native URL instead
  (`Prepared::into_parts`, then `Request::url_mut`).
- Only surviving contributions are formatted; a replaced or removed value's `Display` never runs.
- Header names must be valid field-name tokens; `host`, `content-length`, `transfer-encoding`,
  `connection`, `te`, `trailer`, `upgrade` and `keep-alive` are rejected (transport-owned).
  `Authorization`, `Cookie` and all other names are ordinary.
- Header values must be visible ASCII, space or tab (policy: no obs-text). Literal text is checked
  at compile time; formatted values at build time, as a build error naming the header, not the
  value. The same applies to an `Encode::CONTENT_TYPE`.
- `remove` and the absence of a key concern Concord's contributions only: headers configured on the
  `reqwest::Client` (default headers, user agent) are added by reqwest when the request lacks them.

## 6. Requests and execution

- `endpoint.prepare()` builds a `Prepared<T>` (`T` = the declared output type, `()` without `->`)
  holding the application's `reqwest::Client`, a native `reqwest::Request` and the response
  handling. No I/O happens.
- `Prepared::send()` executes once with `Client::execute`, then applies the endpoint's response
  handling; `endpoint.send()` is `prepare()?.send()`. `Prepared::send_raw()` executes once and
  returns the native response unchecked. `Prepared::into_parts()` returns
  `(reqwest::Client, reqwest::Request, Decoder<T>)`; `Decoder::decode(response)` applies the same
  response handling to any native response, and `Decoder::finish(client.execute(request).await)`
  also converts a native transport error into Concord's `ErrorKind::Transport` (same redaction
  as `send`). `Prepared::request()` gives read-only access.
- Concord performs no retries, no authentication, no rate limiting, no pagination and has no
  transport settings. One `send` is one `Client::execute` call; redirects, proxies, timeouts,
  compression and default headers are whatever the reqwest client does.
- Order of build checks: body encoding (including the representation's content type), then
  subdomain labels and the assembled host (IP base, 253-byte limit), path segments, query and
  headers; the first failure is returned and later values are not formatted.
- A value whose `Display` returns `fmt::Error` (a broken `Display`) is a build error naming the
  placement, in every placement; no partial text is sent. Concord's generated `Debug` shows it as
  `<Display failed>`.

## 7. Bodies and responses

A body is declared `body b: T via R` and a typed response `-> T via R`: `T` is the payload type
the caller supplies or receives, `R` a representation marker implementing `Encode<T>` (bodies) or
`Decode<T>` (responses). Generated signatures use `T` directly. There is no default
representation and no inference from `T`.

| Representation | As body (`Encode<T>`) | As response (`Decode<T>`) |
|---|---|---|
| `Json` (feature `json`) | any `T: Serialize`, `serde_json` bytes, `Content-Type: application/json` | any `T: DeserializeOwned`, `serde_json` from the body; `Content-Type` ignored |
| `Text` | `String`, `text/plain; charset=utf-8` | `String`; invalid UTF-8 is a decode error |
| `Binary` | `bytes::Bytes` or `Vec<u8>`, `application/octet-stream` | `bytes::Bytes` or `Vec<u8>`, unchanged |
| *(no `->`)* | — | output `()`: any body is drained chunk by chunk without buffering |

- An endpoint without `->` has output `()`. A body's `Content-Type` is added only if the composed
  headers have none. Concord never adds `Accept`.
- Response handling: a non-2xx status is `ErrorKind::Status` carrying the **unread**
  `reqwest::Response` (`Error::into_response`); otherwise the body is read and decoded.
  `Option<T>` outputs have no status meaning: a 404 is a status error, JSON `null` is `None`.
- `Encode<T>` and `Decode<T>` are public traits; a representation is chosen only by naming it
  after `via`. `<R as Decode<T>>::decode(bytes)` can also be called directly (for example on the
  body of an error response).
- Observed transport detail: hyper sends a zero-length body without `Content-Length`, so "no body"
  and "empty `Text` body" differ on the wire only by `Content-Type`.

## 8. Errors

`concord::Error` has `kind()` ∈ {`Build`, `Transport`, `Status`, `Decode`}, `endpoint()`
(`Client.scope.endpoint`), `status()`, `into_response()` (`Ok(unread response)` for a status
error, `Err(self)` otherwise), `transport()` and `body_source()`.

- `Display` is `METHOD Client.scope.endpoint: <stage message>` and contains only static text: the
  failing placement's template (e.g. ``path segment `{id}` ``), the header name, the status code and
  its canonical reason, a decoder's content-free description (JSON category and line/column).
- `source()` is always `None`. Underlying errors are reachable only through explicit accessors:
  `transport()` (the reqwest error with its top-level URL removed) and `body_source()`. They are
  documented as unredacted: reqwest nests errors that keep URLs (a refused redirect keeps its
  target, which a server can build from any request value, headers included), TLS errors print
  host names, and decoders quote bodies.

## 9. Sensitivity

Scope: output **produced by Concord** — generated `Debug` of clients, scopes and endpoints;
`Prepared`'s `Debug`; `Error`'s `Display` and `Debug` (it has no `source()` chain). Sensitive values are always
sent on the wire.

- A value is marked with `#[sensitive]`.
- A query/header **key** is sensitive for an endpoint if any contribution to it on the chain reads a
  marked value, or if its name (lowercased, `_` → `-`) is exactly one of: `authorization`,
  `proxy-authorization`, `cookie`, `set-cookie`, `x-api-key`, `api-key`, `apikey`, `x-auth-token`,
  `auth-token`, `access-token`, `refresh-token`, `id-token`, `token`, `client-secret`, `secret`,
  `password`, `signature`, `session`. Sensitivity is sticky: a later public override, removal or
  reintroduction does not clear it.
- A path segment or host label is sensitive if any of its holes reads a marked value.
- **Effective sensitivity** is computed to a fixpoint at compile time: a value is withheld if it is
  marked or feeds a sensitive key on any chain, and every key, path segment and host label that
  reads a withheld value is itself sensitive. Generated `Debug` withholds such values; `Prepared`
  shows sensitive keys' values, segments and labels as `<redacted>`; sensitive header values are
  additionally flagged with `HeaderValue::set_sensitive`.
- **Outside the guarantee:** the native `reqwest::Request`/`Url`/`Response` `Debug` (it prints the
  full URL), reqwest errors from requests the application executes itself, redirects forwarding
  custom headers and `Referer` with the query, tracing, middleware, proxies and server logs, and the
  base URL (never treated as sensitive).
