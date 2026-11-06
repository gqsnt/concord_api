# Concord V4 semantics

Normative description of the implemented language and runtime. Rationale is in `DECISIONS.md`;
executable evidence is listed in `TEST_MATRIX.md`.

## 1. Declaration

```text
api!       := doc* vis "client" Name params? "{" "base" STRING ";" stmt* child* "}"
child      := cattr* "scope" name params? "{" stmt* child* "}"
            | cattr* METHOD name params ("->" Type)? (";" | "{" stmt* "}")
cattr      := doc | "#[cfg(..)]" | "#[deprecated..]"
METHOD     := GET | POST | PUT | PATCH | DELETE | HEAD | OPTIONS
params     := "(" (param ("," param)* ","?)? ")"
param      := attr* placement? name "?"? ":" Type ("=" Expr)?
            | doc* "body" name "?"? ":" Type          (endpoints only, at most one)
placement  := "query" ("(" STRING ")")? "each"? | "header" "(" key ")" "each"?
key        := STRING | Path                   (a Path names a native `HeaderName`; headers only)
attr       := doc | "#[sensitive]"
stmt       := "path" STRING ";" | "subdomain" STRING ";"
            | "query" "{" entry* "}" | "headers" "{" entry* "}"
            | "format" fmt ";" | "request" fmt STRING? ";" | "response" fmt ";"
fmt        := "json" | "text" | "bytes" | "form" | TypePath
entry      := op? key "=" rhs ("else" "inherit")? ";" | "remove" key ";"
op         := "append" | "override"
rhs        := STRING                      (template)
            | "each"? reference
reference  := name | owner "." name       (owner = `client` or an enclosing scope's name)
```

- One `api!` declares exactly one client. Each owner (client, scope, endpoint) declares each
  statement at most once (`format` counts as `request` and `response`), and on clients and scopes
  statements precede child declarations.
- Attributes: documentation everywhere; `#[sensitive]` on values; on scopes and endpoints also
  `#[cfg(..)]` (applied to every generated item of that owner and its descendants, and to the
  parent's navigation method) and `#[deprecated..]` (applied to the navigation method, so the
  warning appears where callers navigate). Other attributes are rejected. Generated names are
  still checked as if every `cfg` were enabled: two cfg-exclusive endpoints cannot share a name.
- Templates (`path`, `subdomain`, quoted query/header values) contain literal text and holes
  `{name}` / `{owner.name}`; `{{` and `}}` are literal braces (with or without holes).
- No meaning is derived from Rust type spellings. `Option<T>` and `Vec<T>` are ordinary types.
  Formats and their statements are described in §7. A header key path is never read by its
  identifier: it is resolved by rustc and used as the native `HeaderName` it names (§5).

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
| `body b: T` | `T` | — | positional argument; then `set_b(v)` |
| `body b?: T` | `Option<T>` | absent | `set_b(v)`, `maybe_b(o)` |

- Entry calls: `Client::new(http: reqwest::Client, config: {Client}Config)` when the client has
  required values (`Client::new(http)` otherwise); `parent.scope(...)`, `parent.endpoint(...)`.
  `{Client}Config` is a generated struct with one public field per required client value, in
  declaration order; its `Debug` redacts like the client's. Scope and endpoint positional
  arguments are the required values and a required body, **in declaration order**.
- Positional arguments, `set_*` and initializers accept `impl IntoValue<T>` (config fields are
  exactly `T`): exactly `T`, `&str`/`&String` for `String`, or `&HeaderValue` for `HeaderValue`
  (cloned). Nothing else converts (no `Into`, no numeric widening, no
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
case-sensitively for query and by native identity for headers: a header key is a literal name,
normalized by `http::HeaderName::from_bytes` (lowercase), or a path to a native `HeaderName`
constant or static; two keys name the same header when their `HeaderName`s are equal.

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
- **When composition is decided.** If every header key on an endpoint's chain is a literal, the
  macro composes the headers and every rule violation is a compile error. If a native constant is
  on the chain, the macro cannot evaluate it: two literals are known to be the same header or
  not, any pair involving a native path is unknown — also two paths written with the same tokens,
  which can resolve to different constants (`$crate::KEY` from two crates' macros). Violations the
  macro's per-pair analysis proves are compile errors (a violation that holds only through a
  correlation between unknown pairs is not proved, and is found at run time instead); the others are checked when the request is built, by
  `__private::HeaderPlan`, which applies the same rules to the native names (equality only, no
  value converted or formatted) and reports a `Build` error naming the declaration. The result —
  which contributions are sent, their order, their classification — is the same in both modes
  (`tests/header_plan.rs` checks the run-time plan against the sequential rule on every chain of up
  to four contributions). Only the order *between different header names* may differ, which is
  not part of the contract.
- Liveness and slot positions are decided by the declaration, never by which optional values are
  present at runtime: an introduction that reads an absent optional value
  sends nothing, yet the key is live (so a descendant may `override` it, not introduce it) and
  keeps its slot position. (Confirmed by the independent composition oracle,
  `tests/oracle_composition.rs`.)
- A source is **present** when every optional value it reads is present. A present `each` source
  with zero items is present and emits nothing — so it still replaces under `override`.
- `each` iterates any `IntoIterator` (`&T: IntoIterator`; items `Display` for query keys,
  `ToHeaderValue` for headers). On a required
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
- Only surviving contributions are formatted or converted; a replaced or removed value's `Display`
  (or `ToHeaderValue`) never runs.
- Header names: a literal must be accepted by `http::HeaderName::from_bytes` (a compile error
  otherwise); a path must name a `HeaderName` (`__private::NativeKey` gives the compile error).
  `host`, `content-length`, `transfer-encoding`, `connection`, `te`, `trailer`, `upgrade` and
  `keep-alive` are rejected (transport-owned): at compile time for a literal, as a `Build` error
  for a native constant. `Authorization`, `Cookie` and all other names are ordinary.
- **Header values** enter the native header map by one of three routes, decided by the
  declaration and the value's type, never by a type's name:
  - *literal text* (a template without holes, `ACCEPT = "application/json"`): checked when the
    macro expands with `http::HeaderValue::from_bytes` (after Rust escapes and `{{ }}` are
    decoded). A visible-ASCII literal is lowered to one generated
    `static HeaderValue = HeaderValue::from_static(..)` per distinct literal (requests clone it);
    any other accepted literal is built per request with the checked constructor.
  - *formatted text* (a template with holes): each literal fragment is checked at expansion (a
    rejected byte in a fragment is a compile error no hole can repair); holes are formatted with
    `Display` and the result goes through `HeaderValue::from_str`, else a build error naming the
    header, not the value;
  - *a value placed directly* (`header(K) x: T`, `K = x`, `K = each xs`): converted with
    `ToHeaderValue`, a sealed trait implemented for `HeaderValue` (cloned and sent byte for byte,
    any bytes `http` accepts, with its sensitivity flag), `str`/`String` (`HeaderValue::from_str`:
    UTF-8 beyond ASCII and horizontal tab accepted, every other control byte rejected; `http`
    decides), the integers and `bool`, and
    `&T`/`Box`/`Rc`/`Arc`/`Cow` of these. A conversion that fails is a build error naming the header.
  Concord keeps no validator of its own: `http` decides every name and value.
- A native `HeaderValue` is never text: in a path, subdomain, query value or template hole it is a
  compile error at the value's type (Concord's note names the explicit conversion).
- Since `ToHeaderValue` is sealed, the only value type that can carry a native sensitivity flag is
  `HeaderValue`, which is never text: no value can be protected in its header placement by a flag
  its text placements would not see.
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
- `Prepared::receive()` executes once and returns a `Received<T>` for **any** status, body unread,
  keeping the response handling and the endpoint's identity (`Operation`: declared name and
  method). Only a transport failure is an error (`ErrorKind::Transport`, naming the endpoint).
  `Received::head()` borrows status, version and headers without consuming anything;
  `Received::decode()` applies the response handling (`send()` is `receive()` then `decode()`, so
  the two paths cannot diverge); `Received::into_incoming()` requires a 2xx and returns the unread
  body with its identity for a reader chosen at run time; `into_response()`/`into_parts()` are the
  native escape. Each consumer takes `self`: one response, one consumer. `Debug` of `Received`
  and `Incoming` prints the operation, status, version and header names, never the URL or a
  header value. Metadata that must survive a body failure is read from the head before the
  consumer into a value the SDK owns; Concord copies no header by default.
- For a request edited natively after `into_parts()`, `Decoder::execute(&client, request)`
  returns the same `Received<T>`; `Decoder::transport_error(&self, e)` converts a native
  transport failure synchronously; `Decoder::receive(response)` attaches the handling to a
  response obtained otherwise. `Incoming::new(response, operation)` status-checks a natively
  obtained response (a signed URL) with an optional static label (`Operation::label`), which
  errors and readers print; `Error::endpoint()` names declared endpoints only,
  `Error::operation()` either.
- Every generated endpoint type implements the public `Endpoint` trait (`type Output`, the
  generated `prepare()`, `operation()` — the declared identity without building a request); it adds
  nothing to the generated namespace. `Received::bytes_limited(n)` reads the body of any status up
  to `n` bytes (`ErrorKind::Limit` beyond, the rest unread), without the declared handling.
- Concord performs no retries, no authentication, no rate limiting, no pagination and has no
  transport settings. One `send` is one `Client::execute` call; redirects, proxies, timeouts,
  compression and default headers are whatever the reqwest client does.
- Order of build checks: body encoding (the media type is a compile-time constant), then
  subdomain labels and the assembled host (IP base, 253-byte limit), path segments, query and
  headers; the first failure is returned and later values are not formatted.
- A value whose `Display` returns `fmt::Error` (a broken `Display`) is a build error naming the
  placement, in every placement that formats (paths, labels, query values, templates); no partial text is sent. Concord's generated `Debug` shows it as
  `<Display failed>`.

## 7. Bodies and responses

Every body and every typed output has a **format**, given by a statement. `T` is the payload type
the caller supplies or receives (generated signatures use it directly); the format is a type
implementing `Encode<T>` (bodies), `Decode<T>` (outputs) or `Incremental<T>` (stream outputs, below),
named by its Rust path. No format is
chosen from the spelling of `T`, and none is known to the macro: the standard formats
(`concord-extras`, re-exported by `concord`) are ordinary implementations.

| Format | As body (`Encode<T>`) | As output (`Decode<T>`): head check, before the body is read | `Accept` suggestion |
|---|---|---|---|
| `Json` (feature `json`) | any `T: Serialize`, `application/json` | any `T: DeserializeOwned`; exactly one `Content-Type`, `application/json` or `application/*+json`, charset absent or once and UTF-8 or US-ASCII | `application/json` |
| `Text` | `String`, `text/plain; charset=utf-8` | `String` (UTF-8); any media type; a declared non-UTF-8 charset (repeated or not) is refused | none |
| `Binary` | `bytes::Bytes` or `Vec<u8>`, `application/octet-stream` | `bytes::Bytes` or `Vec<u8>`, unchanged; nothing checked | none |
| `Form` (feature `form`) | any `T: Serialize` (`serde_urlencoded`), `application/x-www-form-urlencoded` | any `T: DeserializeOwned`; exactly one `application/x-www-form-urlencoded`, charset absent or once and UTF-8 or US-ASCII | `application/x-www-form-urlencoded` |
| `IgnoreContentType<F>` | `F`'s | `F`'s, without `F`'s `Content-Type` check | `F`'s |
| a type path | its `Encode<T>` | its `Decode<T>` | its `Decode::ACCEPT` |
| *(no `->`)* | — | output `()`: any 2xx; nothing checked; the body is drained chunk by chunk without buffering | none |

**Resolution.**

- `request F;`, `request F "m";`, `response F;` and `format F;` (both directions) may be written by
  the client, a scope or an endpoint. For an endpoint's body, the nearest `request` on its chain
  wins (the endpoint's own, then its scopes inside out, then the client); likewise `response` for
  its typed output; the directions are independent. A statement replaces the format **and** the
  media type. No statement reaching a body or typed output is a compile error at its type. On an
  endpoint, `request` without a body or `response` without `->` is a compile error; `format`
  needs one of the two. Statements on clients and scopes that nothing uses are allowed. A default
  cannot be cleared.
- A format is a type path, resolved by Rust where the statement is written, like any type
  (`Json` after `use concord::Json;`, `concord::Json`, an alias, a generic adapter). No name is
  reserved; an unknown name is rustc's unresolved-type error at the name.
- A client or scope statement is referenced through one hidden type alias, emitted only if some
  endpoint consumes it and gated by the union of its consumers' effective `cfg`s: a statement used
  by cfg-disabled endpoints only, or by none, is never resolved.
- An unsupported pair is reported as `the format F cannot encode a T request body` /
  `cannot decode a T response` (`Encode<T>`/`Decode<T>` with Concord's message).

**Content type.**

- A body's media type is the one its `request F "m";` statement declares, or the format's
  `Encode::CONTENT_TYPE`. Declared media types are parsed by the `mime` crate and must be static
  header values for `http` (visible ASCII) — Concord keeps no media-type grammar; both kinds become compile-time
  `HeaderValue::from_static` values (an invalid `CONTENT_TYPE` is a compile error where it may be
  sent and is not evaluated where a declared media type replaces it).
- The media type is sent exactly when the body is. A `content-type` header contribution that
  survives composition on the endpoint (conditional ones included) is sent whether or not a body
  is, and replaces a format's `CONTENT_TYPE`: no body and no header → none; no body + header →
  the header; body + no header → the media type; body + header → the header. An empty native value
  counts as present; an `each` over no items contributes nothing; a conversion failure is a build
  error.
- A declared body media type and a `content-type` contribution surviving on the same endpoint are
  a compile error (a `Build` error when only a native constant can be `content-type`); a
  `remove CONTENT_TYPE` that drops every inherited contribution resolves it.
- The inserted `Content-Type` is flagged sensitive when the `content-type` header is sensitive on
  the endpoint (marker or sticky composition, also after a `remove`), decided at compile time for
  literal chains and by the run-time plan otherwise.
- **Runtime media.** `Encode::encode_body` (defaulted: `encode` with no runtime media type) may
  return an `EncodedBody` whose media type is bound to its bytes at run time (a generated
  multipart boundary, a CSV dialect chosen per job); Concord always calls `encode_body`. Against
  the declaration, when the request is prepared and before any I/O: no declaration → the runtime
  value is sent (it supersedes the format's own `CONTENT_TYPE`); a declared `request F "m";` must
  equal it byte for byte, else a `Build` error; a surviving `content-type` header contribution
  must be exactly one value equal to it, else a `Build` error (an equal one is sent once); a chain
  ending in `remove content-type` sends the runtime value; an absent optional body sends no media
  type. Concord parses no media type (byte equality is the documented tradeoff). The body's
  length is the body's own: a known size hint is sent with `Content-Length`, a stream of unknown
  length chunked; Concord never buffers a body to learn its length. The standard `Encoded` format
  passes an `EncodedBody` or a runtime writer's `Upload` through a declared body (`body x: Upload`
  with `request Encoded;`), so a body-carrying operation is declared with its body.
- An output takes no media type: the response `Content-Type` is the format's to check
  (`Decode::check_content_type`), never compared with the request's `Accept`. Concord does not
  enforce the request field's cardinality (repeated declared `content-type` values are the
  author's choice).

**Accept.**

- A format may suggest an `Accept` (`Decode::ACCEPT`, a compile-time-checked static value). It is
  sent only for an endpoint with a typed output, after the authored headers are composed, when no
  `accept` header was composed and the chain's last `accept` contribution is not a `remove`. It
  never takes part in composition (it is not a `remove` target, cannot collide, cannot make a
  `remove` legal). An absent optional authored value leaves the suggestion; a removal suppresses
  it; a contribution after the removal reopens it. Authored values (literal, native, empty,
  repeated, with q-values) are sent as written. It is flagged sensitive when `accept` is sensitive
  on the endpoint, like the inserted `Content-Type`.
- At execution reqwest fills only header names the request lacks from the client's defaults (its
  own default is `Accept: */*`): a composed or suggested `Accept` replaces a client default; when
  Concord sends none, the client's default goes out.

**Presence and decoding.**

- `body b?: T` absent sends no body and no format media type; a required `Option` body under `Json`
  is `null`; an empty `Text`/`Binary` body is present (with its media type). A body cannot be
  initialized.
- Without `->` the output is `()`, any 2xx succeeds and the body is drained; a `response`
  statement does not apply. `-> ()` (or an alias of it) is a typed output decoded with its format: under
  `Json` the body must be `null`; an empty 204 without `Content-Type` is refused by the head check,
  and labelled JSON it is a decode error.
- Response handling, in order: a non-2xx status is `ErrorKind::Status` carrying the **unread**
  `reqwest::Response`; on a 2xx the format checks the head (`check_content_type`, then `context`,
  which produces the owned state the body decoder needs) and a refusal is `ErrorKind::Metadata`
  carrying the unread response; then the body is read and `decode_body(context, bytes)` decodes it
  (`ErrorKind::Decode` on failure). `Option<T>` outputs have no status meaning: a 404 is a status
  error, JSON `null` is `None`. `send_raw()` checks nothing; `into_parts()` + `Decoder::finish` and
  `Decoder::new::<F>()` apply the same rules.
- `Encode<T>` and `Decode<T>` are public, open traits; `<F as Decode<T>>::decode_body(context, bytes)`
  can be called directly (for example on the body of an error response), with no head check.
- Observed transport detail: hyper sends a zero-length body without `Content-Length`, so "no body"
  and "empty `text` body" differ on the wire only by `Content-Type`.

**Incremental outputs.**

- `-> stream T` declares an output read as the response arrives. Its format is the nearest
  `response` statement, which must implement `Incremental<T>` (`Csv<..>`, `JsonLines`, `Ndjson`,
  `JsonStream`, `IgnoreContentType<F>`, or yours); `send()` returns the format's reader
  (`Records<F, T>`, e.g. `CsvRecords<T>`) after the status and head checks, **without reading the
  body**. `-> stream` without an item returns the unread `Incoming` body, any 2xx, no format (a
  `response` statement there is a compile error). Neither drains; the drain applies only without
  `->`. `stream` is the keyword unless it begins a path or a generic type (`-> stream::E`,
  `-> stream<T>`); `r#stream` is a type.
- A reader opens an `Incoming`, a `Received<T>` (status checked) or a native response, checks the
  head before polling the body, and names the declared operation (or label) in every later error,
  without a string written by the SDK. `completion()` is `EndOfInput` only after the clean end of
  the input; a terminal error is `Failed`; dropping or stopping early is neither. `into_stream()`
  gives an owning `futures_core::Stream` (`RecordStream`) with the same errors, no read-ahead and a
  cancellation-safe pending read.

| Stage | Finite `-> T` (and `()`) | `-> stream T` / `-> stream` + reader |
|---|---|---|
| request not built | `Build`, nothing sent | same |
| no response | `Transport`, no response | same |
| non-2xx | `Status`, response unread | `Status` (`send()`), or reader `OpenError::Status`, response unread |
| head refused | `Metadata`, response unread | `Metadata` (`send()`), or reader `OpenError::Metadata`, unread |
| body read fails | `Transport` with the endpoint; the bytes are gone; SDK metadata read from the head survives | `ReadError` `Transport`, terminal, with position; records already delivered stay delivered |
| body malformed / does not decode | `Decode` | `ReadError` `Format`/`Decode`: terminal, except JSON Lines `RecordErrors::Yield` line errors (recoverable, `is_terminal() == false`) |
| limit | — | `ReadError` `Limit`, terminal, never a clean end |
| caller stops / drops | the request future is dropped | the reader is dropped: `completion()` was `Reading`, not `EndOfInput` |

## 8. Errors

`concord::Error` has `kind()` ∈ {`Build`, `Transport`, `Status`, `Metadata`, `Decode`},
`endpoint()` (`Client.scope.endpoint`), `status()` (`Status` only), `response()` and
`into_response()` (the unread response of a `Status` or `Metadata` error; `Err(self)` otherwise),
`codec_error()` (`CodecError`: a static label and up to three numeric positions) and `transport()`.

- `Display` is `METHOD Client.scope.endpoint: <stage message>` and contains only static text: the
  failing placement's template (e.g. ``path segment `{id}` ``), the header name, the status code and
  its canonical reason, a format's static `CodecError` label and its numeric positions.
- `source()` is always `None`. Underlying errors are reachable only through explicit accessors:
  `transport()` (the reqwest error with its top-level URL removed) and
  `codec_error()?.source_unredacted()`. They are
  documented as unredacted: reqwest nests errors that keep URLs (a refused redirect keeps its
  target, which a server can build from any request value, headers included), TLS errors print
  host names, and decoders quote bodies.

## 9. Sensitivity

Scope: output **produced by Concord** — generated `Debug` of clients, scopes and endpoints;
`Prepared`'s `Debug`; `Error`'s `Display` and `Debug` (it has no `source()` chain). Sensitive values are always
sent on the wire.

- A value is marked with `#[sensitive]`.
- A query/header **key** is sensitive for an endpoint if any contribution to it on the chain reads a
  marked value, or if its name (lowercased, `_` → `-`) is exactly one of the well-known names
  listed once, in the author guide (§7, "Sensitive values"; the macro's and the runtime's copies
  are kept equal by a unit test). Sensitivity is sticky: a later public override, removal or
  reintroduction does not clear it.
- A path segment or host label is sensitive if any of its holes reads a marked value.
- **Native keys.** A key's name is its resolved `HeaderName`: a native constant on the list
  (`AUTHORIZATION`, an alias of it) classifies its values exactly as the literal does, and a
  constant whose identifier looks sensitive but whose name is not on the list does not. Because
  the macro cannot evaluate a constant, the compile-time fixpoint treats each native key
  occurrence as a header of its own, not on the list (a lower bound); a value it leaves unclassified that is sent
  under a native constant is also withheld at run time — in the generated `Debug` (decided when
  rendering) and in the outgoing flag (decided by the request's header plan) — when the resolved
  name is on the list or names the same header as a sensitive contribution. Such a value that is
  also written as text (path, subdomain, query, template) is withheld statically instead, since
  URL redaction is static.
- **Effective sensitivity** is computed to a fixpoint at compile time: a value is withheld if it is
  marked or feeds a sensitive key on any chain, and every key, path segment and host label that
  reads a withheld value is itself sensitive. Generated `Debug` withholds such values; `Prepared`
  shows sensitive keys' values, segments and labels as `<redacted>`; sensitive header values are
  additionally flagged with `HeaderValue::set_sensitive`.
- **Native sensitivity** is part of a `HeaderValue`. Each outgoing header value's flag is
  *its own native flag OR the key's classification above*: Concord sets the flag, never clears it,
  and sets it on its own clone (a caller's or a `static`'s value is never mutated). The rule is per
  value: in an `each` list each item keeps its own flag; an `append`ed value does not flag its
  siblings, and a value removed or replaced by `override`/`remove` is simply not sent. Runtime
  flags never change the compile-time classification (no runtime propagation to other keys,
  segments or values). Concord's output honours the flag wherever a header value is shown:
  `Prepared`'s `Debug` prints `name: <redacted>`, and the generated `Debug` of a value placed only
  as a header shows `<redacted>` for a flagged value (per item for `each`).
- An application that extracts the bytes of a native value into another value (for example
  `value.to_str()` into a `String`) creates new data: its protection is the declaration's
  (`#[sensitive]`, key name), not the original flag.
- **Outside the guarantee:** the native `reqwest::Request`/`Url`/`Response` `Debug` (it prints the
  full URL), reqwest errors from requests the application executes itself, redirects forwarding
  custom headers and `Referer` with the query, tracing, middleware, proxies and server logs, and the
  base URL (never treated as sensitive).
