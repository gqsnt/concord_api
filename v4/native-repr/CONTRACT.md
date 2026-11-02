# Body and response formats — contract (written before implementation)

The rules the implementation must satisfy, with one test per row (`T:` names the test that pins
it; filled in as tests land). Terminology: a **format** is what the declaration names (`json`,
`text`, `bytes`, `form`, or a type path); its runtime is a marker type implementing
`Encode<T>`/`Decode<T>` (`Json`, `Text`, `Binary`, `Form`, or the user's type).

## Grammar

```text
owner statements (client, scope; at most once each, before children):
    request FORMAT;              request FORMAT("media/type");
    response FORMAT;
body (in an endpoint's parameter list, at most one; `?` makes it optional):
    body name: T                 the nearest `request` default (format and media type)
    body("media/type") name: T   the nearest default format, this media type
    FORMAT name: T               this format, its own media type
    FORMAT("media/type") name: T this format, this media type
output:
    (no `->`)                    no typed output
    -> T                         decoded with the nearest `response` default
    -> FORMAT T                  decoded with this format
FORMAT:  json | text | bytes | form | TypePath
```

A bare lowercase identifier in format position must be one of the four words (a lowercase format
type is written as a path, `self::csv`). Words resolve to `$crate::{Json, Text, Binary, Form}`, so
they need no import and work under a renamed dependency. A type path resolves at the declaration
site like any type.

Lexical precedence (each pinned by a test):

| Fragment | Meaning |
|---|---|
| `json: T`, `json?: T` | an ordinary value named `json` |
| `header("x") json: T` | a header value named `json` |
| `json query: T` | a JSON body named `query` |
| `self::csv rows: T`, `json::Codec rows: T` | a custom format path (not the word `json`) |
| `-> Vec<Item> {` / `-> Vec<Item>;` | the inherited format |
| `-> codecs::Csv Vec<Item>`, `-> codecs::Csv ()` | an explicit custom format |
| `-> json {` | error: the payload type is missing (words are reserved in format position) |
| `-> r#json` | a payload type named `json` (a raw identifier is never a word) |

## Truth table

| # | Situation | Behaviour | Diagnostic / note |
|---|---|---|---|
| 1 | Endpoint without a body under a `request` default | no body; the format adds no `Content-Type` (declared headers and the reqwest client's default headers still apply) | — |
| 2a | `body d: Option<Discount>` (required), `None`, JSON | body `null`, `Content-Type: application/json` | — |
| 2b | `body q?: Q` absent (`maybe_q(None)` / never set) | no body; the format and the body media type add no `Content-Type` | — |
| 2c | `text note: String` = `""`; `bytes b: Bytes` = empty | present empty body, `Content-Type` of the format | — |
| 2d | `body b: T = expr` | rejected | "a body cannot have a default" |
| 3a | No `->` under a `response` default | any 2xx succeeds, body drained unbuffered, output `()`; the default does not apply | rustdoc says so |
| 3b | `-> ()` with an inherited format | a typed unit decoded with that format, exactly like `-> Unit` for `type Unit = ()` (JSON: the body must be `null`; an empty 204 is a decode error) | rustdoc of such an endpoint says so and that no output is written without `->` |
| 3c | `-> json ()` | decodes JSON `null`; an empty 204 is a decode error | — |
| 4a | `-> String` under `response json;` | JSON string (`"2.4.1"` → `2.4.1`; raw `2.4.1` is a decode error) | rustdoc names the format and origin |
| 4b | `-> text String` | raw UTF-8 text | — |
| 5 | Aliases (`type Raw = String`, `type Unit = ()`, a payload named `JsonDoc`, `type Wire = concord::Text` as a format path) | behaviour and acceptance follow the written format and the owner defaults only | the macro reads spellings only to choose hints in errors it reports anyway (`via`, alpha `Json<T>`) |
| 6a | Defaults | request and response independent; nearest owner on the endpoint's chain wins | — |
| 6b | Endpoint exception | written in place; replaces the inherited format (and its media type) for that body/output | — |
| 6c | `request`/`response` statement on an endpoint | compile error | "write the format where the body/output is declared" |
| 6d | Clearing a default | not provided: under a client default a scope cannot demand explicit formats; put defaults on uniform scopes instead of the client, or write every exception in place | — |
| 7 | `body x: T` / `-> T` with no default on the chain | compile error at the payload | names the in-place word and the owner statement |
| 8a | Content type of a body | the body's media type: explicit (`FORMAT("…")`, `body("…")`, or a `request` default's `("…")` reached through `body`) or the format's `Encode::CONTENT_TYPE`; sent exactly when the body is. An in-place format word replaces the default's format **and** its media type | — |
| 8b | `content-type` header contributions surviving composition on the endpoint (including conditional ones), body media type implicit | runtime: no body + no header → none; no body + header → the header; body + no header → the format's media type; body + header → the header (an empty native value counts as present; an empty `each` contributes nothing; a conversion failure stays a build error). The header is sent whether or not a body is present | rustdoc says so |
| 8c | Surviving `content-type` contributions and an explicit body media type on one endpoint | compile error; a `remove "content-type"` that eliminates every inherited contribution removes the conflict | "keep one" |
| 8f | Sensitivity of the inserted `Content-Type` | the effective static classification of the `content-type` key on that endpoint (marker, sticky composition) is applied to the inserted value too | — |
| 8g | Validation | an explicit media type is checked by the macro (visible ASCII; `type/subtype` tokens, optional `; parameters`); a format's `Encode::CONTENT_TYPE` is validated at compile time wherever it may be sent, and not at all where an explicit media type replaces it. Concord does not enforce the field's cardinality (repeated `content-type` contributions are the author's choice) | — |
| 8d | `Accept` | never added; the response `Content-Type` is never checked | — |
| 8e | Media type on an output (`-> json("…") T`) | compile error | Concord sends no `Accept` and does not check the response type |
| 9 | Custom codec | downstream marker type with `Encode<T>`/`Decode<T>` impls, named by path in place or in a default | errors point at the declaration (the payload or the format) with the trait context; where a blanket impl's bound fails (serde's `DeserializeOwned` for `json`), rustc reports that bound instead of Concord's message |
| 10 | Streaming, per-request content type, multipart | native exits: `send_raw`, `into_parts` + `Decoder::finish`, reqwest's own multipart | — |
| 11 | Status and metadata | unchanged: 2xx required by `send`/`decode`, non-2xx is `ErrorKind::Status` with the unread response, `send_raw` unchecked, decoder retained after native execution; no size limit or bounded drain is promised | — |
| 12a | Built-in word whose feature is off (`json`, `form`) | in place: an error at each word; a default: one error at the default (a shared hidden alias), with rustc's "gated behind the feature" note | — |
| 12b | `#[cfg]`-disabled endpoints and unused defaults | a default is referenced only through an alias gated by the union of its consuming endpoints' effective `cfg`s (no consumer: no alias), so a default used only by disabled endpoints is never referenced — and an unused default is not checked at all | — |
| 12c | Renamed dependency | words need no import; type paths resolve normally | — |
| 12d | Leftover `via` | compile error with the exact replacement | migration script |
| 12e | Generated rustdoc | every endpoint states its body and output format, media type and where an inherited format comes from; owners state their defaults | — |

## Headers (native values)

| # | Situation | Behaviour |
|---|---|---|
| H1 | `header("k") x: HeaderValue` (config, setter, snapshot) | bytes and flag preserved; no `Display`, no reparse; Concord clones |
| H2 | `= ACCEPT.clone()`, `= &ACCEPT`, `set_x(&ACCEPT)` | initializer evaluated once per owner creation (unchanged rule); payload shared with the `static` |
| H3 | optional / initialized native values | as any value |
| H4 | `each` over `Vec<HeaderValue>` | one line per item, same-name order kept, per-item flags |
| H5 | non-UTF-8 bytes | sent byte for byte (raw-byte assertion) |
| H6 | natively flagged, unmarked, unlisted name | flag kept on the wire; redacted in every Concord `Debug` |
| H7 | override / append / remove / `else inherit` | per-value flags; sticky static classification added |
| H8 | aliases, qualified paths, renamed dependencies | trait resolution, no spelling |
| H9 | integers, `bool`, text, own types | integer/`bool`/text impls; own types implement `ToHeaderValue`; `Display` via a template |
| H10 | `HeaderValue` in a path, query or template | compile error at the value's type |
| H11 | optional native header vs body presence | independent |
| H12 | a format default vs header presence and inheritance | independent |
