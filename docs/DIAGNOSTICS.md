# Diagnostics

What the compiler says when a declaration or a call is wrong, and how to fix it. Every example is
taken from the compile-fail tests in
[`crates/concord/tests/ui`](../crates/concord/tests/ui): the declaration is an excerpt of the
test's `.rs` file and the message an excerpt of its `.stderr` snapshot, which `cargo test -p
concord --test ui` compares with the real compiler output.

Two kinds of messages appear:

- **Concord's own** (`error: ...` without a code) are produced by the `api!` macro. They name the
  rule and point at the declaration. Their wording is part of this repository.
- **Compiler messages** (`error[E....]`) come from rustc: missing struct fields, wrong argument
  counts, unmet trait bounds and lints. Concord shapes some of them (trait-bound messages for
  formats and values carry Concord's own text and notes) but their layout, wording and
  sometimes their number depend on the compiler version. The snapshots are pinned to rustc 1.97.1
  (`rust-toolchain.toml`); other versions print variations of them.

None of these messages contains a runtime value: compile-time errors only quote the declaration.

A declaration with an error generates nothing, so code that uses the client elsewhere reports
unresolved names too: fix the `api!` error first. One exception, for the usual state while you
write a declaration: a value that is declared but not yet used is reported and rejected, but the
client is still generated, so its uses keep resolving
([`unused_value_keeps_the_client.rs`](../crates/concord/tests/ui/unused_value_keeps_the_client.rs)).

## Names

### A value of an enclosing owner, used without qualification

A bare name refers to the current owner's values only. Excerpt of
[`ancestor_needs_qualification.rs`](../crates/concord/tests/ui/ancestor_needs_qualification.rs):

```rust
concord::api! {
    pub client Api(locale: String = "en") {
        base "http://x.test";
        query { "l" = locale; }
        GET item() { query { "locale" = locale; } }
    }
}
```

Excerpt of [`ancestor_needs_qualification.stderr`](../crates/concord/tests/ui/ancestor_needs_qualification.stderr):

```text
error: `locale` is not a value of `Api.item`; to use the value of an enclosing owner, write `client.locale`
 --> tests/ui/ancestor_needs_qualification.rs:5:41
  |
5 |         GET item() { query { "locale" = locale; } }
  |                                         ^^^^^^
```

**Fix:** write `client.locale` (or `<scope>.name` for a value of an enclosing scope).

### An unknown value

Excerpt of [`unknown_value.stderr`](../crates/concord/tests/ui/unknown_value.stderr):

```text
error: `idd` is not a value of `Api.item`; did you mean `id`?
 --> tests/ui/unknown_value.rs:4:37
  |
4 |         GET item(id: String) { path "items/{idd}"; query { "id" = id; } }
  |                                     ^^^^^^^^^^^^^
```

**Fix:** correct the name, or declare the value on this owner.

### A Rust constant on the right of `=`

In a block, the left of `=` is a key (a literal, or a Rust path to a `HeaderName` for a header) and
the right is a value: a template or a declared value's name. A Rust constant is neither. Excerpt
of [`header_constant_as_value.stderr`](../crates/concord/tests/ui/header_constant_as_value.stderr):

```text
error: `JSON` is not a value of `Api`; the right of `=` and `{..}` holes name values declared in a parameter list (a Rust constant is not one: declare a value initialized from it, `json: T = ..`)
  --> tests/ui/header_constant_as_value.rs:10:28
   |
10 |         headers { ACCEPT = JSON; }
   |                            ^^^^
```

**Fix:** write the literal (`ACCEPT = "application/json";`), or declare a value initialized from
the constant and place it: `header(ACCEPT) accept: &'static str = JSON`. On the left, a path that is
not a `HeaderName` is rustc's `` `&str` is not a native header name `` from the same fixture.

## Required values

### A missing field of the client's config

Required client values are fields of the generated `{Client}Config`. Excerpt of
[`config_missing_field.rs`](../crates/concord/tests/ui/config_missing_field.rs):

```rust
concord::api! {
    pub client Api(region: String, account: String, project: String) {
        base "http://x.test";
        query { "region" = region; "account" = account; "project" = project; }
        GET item();
    }
}
fn main() {
    let api = Api::new(
        concord::reqwest::Client::new(),
        ApiConfig { region: "eu".into(), project: "p".into() },
    );
    let _ = api.item();
}
```

Excerpt of [`config_missing_field.stderr`](../crates/concord/tests/ui/config_missing_field.stderr)
(compiler message):

```text
error[E0063]: missing field `account` in initializer of `ApiConfig`
  --> tests/ui/config_missing_field.rs:12:9
   |
12 |         ApiConfig { region: "eu".into(), project: "p".into() },
   |         ^^^^^^^^^ missing `account`
```

**Fix:** supply the field. Forgetting the config argument entirely is `E0061` ("argument #2 of
type `ApiConfig` is missing"; [`missing_required_argument.stderr`](../crates/concord/tests/ui/missing_required_argument.stderr)).

### A value of the wrong type

Arguments and setters accept exactly the declared type, `&str`/`&String` for `String`, or
`&HeaderValue` for `HeaderValue`.
Excerpt of [`newtype_is_not_converted.stderr`](../crates/concord/tests/ui/newtype_is_not_converted.stderr)
(compiler message with Concord's text), for `api.user(42u64)` where `user` is declared
`user: UserId`:

```text
error[E0277]: `u64` is not accepted for a value of type `UserId`
  --> tests/ui/newtype_is_not_converted.rs:14:22
   |
14 |     let _ = api.user(42u64);
   |                 ---- ^^^^^ expected `UserId`
   |                 |
   |                 required by a bound introduced by this call
   |
   = help: the trait `IntoValue<UserId>` is not implemented for `u64`
   = note: Concord accepts the exact declared type, `&str`/`&String` for `String`, or `&HeaderValue` for `HeaderValue`; construct newtypes and `Option`s explicitly
```

**Fix:** construct the value: `api.user(UserId(42))`. For an optional value, `maybe_x` takes
exactly `Option<T>`, and only optional values have one
([`only_optional_values_have_maybe.stderr`](../crates/concord/tests/ui/only_optional_values_have_maybe.stderr)).

## Base, headers and wire formatting

### An invalid base URL

The declared base is checked at compile time. Excerpt of
[`base_errors.stderr`](../crates/concord/tests/ui/base_errors.stderr):

```text
error: invalid base URL "https://user@x.test/?q=1": it must not contain a query; declare query parameters with `query`
 --> tests/ui/base_errors.rs:3:14
  |
3 |         base "https://user@x.test/?q=1";
  |              ^^^^^^^^^^^^^^^^^^^^^^^^^^
```

Excerpt of [`base_invalid_port.stderr`](../crates/concord/tests/ui/base_invalid_port.stderr):

```text
error: invalid base URL "http://example.test:not-a-port": the port must be a decimal number from 1 to 65535
 --> tests/ui/base_invalid_port.rs:3:14
  |
3 |         base "http://example.test:not-a-port";
  |              ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

**Fix:** declare `http[s]://host[:port][/path]` with an ASCII host; put query parameters in a
`query` block. A URL outside the static subset (an IDN or punycode host, `%` in the path) can be
passed at runtime with `with_base_url`. A client without `base` is also an error
([`missing_base.stderr`](../crates/concord/tests/ui/missing_base.stderr)).

### An invalid header name or value

Excerpt of [`invalid_header_name.stderr`](../crates/concord/tests/ui/invalid_header_name.stderr):

```text
error: "x tag" is not a valid header name (rejected by `http::HeaderName::from_bytes`)
 --> tests/ui/invalid_header_name.rs:4:19
  |
4 |         headers { "x tag" = "a"; }
  |                   ^^^^^^^
```

Excerpt of [`header_literal_value.stderr`](../crates/concord/tests/ui/header_literal_value.stderr):

```text
error: this header value contains a byte that `http::HeaderValue::from_bytes` rejects (0x0A at byte 1)
 --> tests/ui/header_literal_value.rs:8:41
  |
8 |         GET ctl() { headers { "x-ctl" = "a\nb"; } }
  |                                         ^^^^^^
```

**Fix:** literal names and values are checked by `http` itself when the macro expands
(`HeaderName::from_bytes`, `HeaderValue::from_bytes`, after Rust escapes and `{{ }}` are decoded):
UTF-8 beyond ASCII is a valid value, control bytes are not, and a hole cannot repair an invalid
literal fragment. Transport-owned names such as `host` or `content-length` are rejected too
([`reserved_headers_all.stderr`](../crates/concord/tests/ui/reserved_headers_all.stderr)). A value
formatted at runtime that `http` rejects is a build error from `prepare()`, naming the header. A
native key (`header(AUTHORIZATION)`) must be a `HeaderName` constant or static
([`native_key_type.stderr`](../crates/concord/tests/ui/native_key_type.stderr)).

### A type without `Display`

Path segments, subdomain labels, query values and template holes are written with `Display`.
Excerpt of [`option_has_no_wire_spelling.rs`](../crates/concord/tests/ui/option_has_no_wire_spelling.rs):

```rust
concord::api! {
    pub client Api {
        base "http://x.test";
        GET list(query filter: Option<String>);
    }
}
```

Excerpt of [`option_has_no_wire_spelling.stderr`](../crates/concord/tests/ui/option_has_no_wire_spelling.stderr)
(compiler message; it is reported twice, once for the request and once for the generated `Debug`):

```text
error[E0277]: `Option<String>` is written as text here and has no `Display`
 --> tests/ui/option_has_no_wire_spelling.rs:4:32
  |
4 |         GET list(query filter: Option<String>);
  |                                ^^^^^^ used in a path, subdomain, query value or template
```

**Fix:** for "maybe absent", declare the value optional (`query filter?: String`); for a list, use
`each` ([`vec_needs_each.rs`](../crates/concord/tests/ui/vec_needs_each.rs)); for another spelling,
a newtype with the `Display` you need.

A native `HeaderValue` is not text either: in a query, a path or a template it is the same error
([`native_header_cross_placement.stderr`](../crates/concord/tests/ui/native_header_cross_placement.stderr)),
whose note says so. Concord never decodes header bytes into text implicitly; convert explicitly
(for example `value.to_str()`) into a `String` value if that is what you mean.

### A value that is not a header value

A value placed directly as a header (`header("..") x: T`, `"name" = x`, `"name" = each xs`) needs
`concord::ToHeaderValue`, not `Display`. Excerpt of
[`header_value_type.stderr`](../crates/concord/tests/ui/header_value_type.stderr):

```text
error[E0277]: `IpAddr` cannot be sent as a header value
  --> tests/ui/header_value_type.rs:14:40
   |
14 |         GET foreign(header("x-ip") ip: std::net::IpAddr);
   |                                        ^^^ not a header value
```

**Fix:** a native `HeaderValue`, text, integers and `bool` are accepted as they are
(`ToHeaderValue` is sealed: it cannot be implemented for other types). To send a type's `Display`
text, use a template: `headers { "x-ip" = "{ip}"; }` (the text is checked by
`HeaderValue::from_str` when the request is built); for another encoding, build a `HeaderValue`
and declare the value as one. `Option` and lists take `?` and `each`.

## Bodies and responses

### A body or output without a format

Excerpt of [`representation_missing_via.rs`](../crates/concord/tests/ui/representation_missing_via.rs):

```rust
pub struct Item;
concord::api! {
    pub client Api {
        base "http://x.test";
        request json;
        GET item() -> Item;
    }
}
concord::api! {
    pub client Body {
        base "http://x.test";
        response json;
        POST create(body item: Item);
    }
}
fn main() {}
```

Excerpt of [`representation_missing_via.stderr`](../crates/concord/tests/ui/representation_missing_via.stderr):

```text
error: output `Item` has no representation: say `response json;` on this endpoint, a scope or the client; omit `->` to drain
 --> tests/ui/representation_missing_via.rs:9:23
  |
9 |         GET item() -> Item;
  |                       ^^^^

error: body `item` has no representation: say `request json;` on this endpoint, a scope or the client
  --> tests/ui/representation_missing_via.rs:16:32
   |
16 |         POST create(body item: Item);
   |                                ^^^^
```

**Fix:** say how it travels with a statement on the endpoint, an enclosing scope or the client: `response json;`, `request json;` (or `format json;` for both). The two directions are independent: a `request` statement gives outputs no format. An endpoint with no typed output has no `->` at all; `-> ()` is a typed output decoded with its format.

### A removed in-place format

Excerpt of [`format_swapped_order.rs`](../crates/concord/tests/ui/format_swapped_order.rs):

```rust
concord::api! {
    pub client Output {
        base "http://x.test";
        GET item() -> Vec<Item> json;
    }
}
concord::api! {
    pub client Body {
        base "http://x.test";
        POST create(item: Item json);
    }
}
```

Excerpt of [`format_swapped_order.stderr`](../crates/concord/tests/ui/format_swapped_order.stderr):

```text
error: removed output format: write `-> Vec<Item>` and `response json;` on this endpoint
 --> tests/ui/format_swapped_order.rs:7:33
  |
7 |         GET item() -> Vec<Item> json;
  |                                 ^^^^

error: removed body format: write `body item: Item` and `request json;` on this endpoint
  --> tests/ui/format_swapped_order.rs:13:32
   |
13 |         POST create(item: Item json);
   |                                ^^^^
```

**Fix:** the payload position names the type only; write the format as a statement on the endpoint, as the message says (or rely on a statement of a scope or the client). The same message names the replacement for every removed spelling — `json b: T`, `json("m") b: T`, `body("m") b: T`, `-> text T`, `request json("m");` ([`format_removed_spellings.stderr`](../crates/concord/tests/ui/format_removed_spellings.stderr)), the V4-alpha `-> concord::Json<Item>` ([`representation_alpha_spelling.stderr`](../crates/concord/tests/ui/representation_alpha_spelling.stderr)); see [From in-place formats to statements](MIGRATION.md#from-in-place-formats-to-statements).

### A leftover `via`

Excerpt of [`format_via_removed.rs`](../crates/concord/tests/ui/format_via_removed.rs):

```rust
concord::api! {
    pub client Output {
        base "http://x.test";
        GET item() -> Item via concord::Json;
    }
}
```

Excerpt of [`format_via_removed.stderr`](../crates/concord/tests/ui/format_via_removed.stderr):

```text
error: `via` was removed: write `-> Item` and `response concord::Json;` on the endpoint
 --> tests/ui/format_via_removed.rs:7:28
  |
7 |         GET item() -> Item via concord::Json;
  |                            ^^^
```

**Fix:** write the statement the message names, keeping the path so it names the same type; where that type is Concord's own `Json`, `Text` or `Binary`, the word is the short form (`response json;`). See [From `via` to statements](MIGRATION.md#from-via-to-statements).

### A statement with nothing to apply to

Excerpt of [`format_endpoint_statement.rs`](../crates/concord/tests/ui/format_endpoint_statement.rs):

```rust
concord::api! {
    pub client RequestWithoutBody {
        base "http://x.test";
        GET version() -> String { request text; response text; }
    }
}
concord::api! {
    pub client ResponseWithoutOutput {
        base "http://x.test";
        POST note(body note: String) { request text; response text; }
    }
}
```

Excerpt of [`format_endpoint_statement.stderr`](../crates/concord/tests/ui/format_endpoint_statement.stderr):

```text
error: `request` statement has no body to apply to
 --> tests/ui/format_endpoint_statement.rs:7:35
  |
7 |         GET version() -> String { request text; response text; }
  |                                   ^^^^^^^

error: `response` statement has no typed output (`-> T`) to apply to
  --> tests/ui/format_endpoint_statement.rs:13:54
   |
13 |         POST note(body note: String) { request text; response text; }
   |                                                      ^^^^^^^^
```

**Fix:** on an endpoint, `request` needs a body (`body x: T`), `response` a typed output (`-> T`), and `format` at least one of the two; remove the statement or declare what it applies to. Each owner states each direction once (`format` counts as both). Statements on clients and scopes that no endpoint uses are fine.

### A media type on an output

Excerpt of [`format_output_media_type.rs`](../crates/concord/tests/ui/format_output_media_type.rs):

```rust
pub struct Item;
concord::api! {
    pub client OnTheEndpoint {
        base "http://x.test";
        GET item() -> Item { response json "application/vnd.api+json"; }
    }
}
concord::api! {
    pub client OnTheClient {
        base "http://x.test";
        response json "application/vnd.api+json";
        GET item() -> Item;
    }
```

Excerpt of [`format_output_media_type.stderr`](../crates/concord/tests/ui/format_output_media_type.stderr):

```text
error: an output takes no media type: Concord sends no `Accept` and does not check the response `Content-Type`; to send an `Accept` header, declare it (`headers { "accept" = "..."; }`)
 --> tests/ui/format_output_media_type.rs:8:44
  |
8 |         GET item() -> Item { response json "application/vnd.api+json"; }
  |                                            ^^^^^^^^^^^^^^^^^^^^^^^^^^

error: an output takes no media type: Concord sends no `Accept` and does not check the response `Content-Type`; to send an `Accept` header, declare it (`headers { "accept" = "..."; }`)
  --> tests/ui/format_output_media_type.rs:14:23
   |
14 |         response json "application/vnd.api+json";
   |                       ^^^^^^^^^^^^^^^^^^^^^^^^^^
```

**Fix:** remove it: Concord sends no `Accept` and checks no response `Content-Type`. If the API needs an `Accept` header, declare it like any header. A body's media type is written on `request` (`request json "application/vnd.api+json";`); `format` takes none.

### A body media type and a `content-type` header

Excerpt of [`format_header_media_conflict.rs`](../crates/concord/tests/ui/format_header_media_conflict.rs):

```rust
concord::api! {
    pub client InPlace {
        base "http://x.test";
        headers { "content-type" = "text/csv"; }
        POST save(body p: String) { request text "text/plain; charset=utf-8"; }
    }
}
```

Excerpt of [`format_header_media_conflict.stderr`](../crates/concord/tests/ui/format_header_media_conflict.stderr):

```text
error: body `p` has the media type "text/plain; charset=utf-8" (written in the endpoint request statement), and the `content-type` header set by `InPlace` reaches this endpoint: both would set `Content-Type`; keep one: remove the media type from the endpoint request statement, or add `remove "content-type";` to this endpoint's `headers` block
 --> tests/ui/format_header_media_conflict.rs:9:50
  |
9 |         POST save(body p: String) { request text "text/plain; charset=utf-8"; }
  |                                                  ^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

**Fix:** keep one. Either the body states its media type (and `remove "content-type";` drops the inherited header on this endpoint or an enclosing scope), or the header does (and the `request` statement names no media type, so the header replaces the format's own). A `content-type` header without a declared body media type is not an error: it replaces the format's media type when it is sent. With a native `CONTENT_TYPE` key the macro may not see that it is `content-type`; the conflict is then a build error from `prepare()`.

### A misspelled or lowercase format

Excerpt of [`format_reserved_words.rs`](../crates/concord/tests/ui/format_reserved_words.rs):

```rust
pub struct Rows;
concord::api! {
    pub client MissingFormat {
        base "http://x.test";
        GET item() -> Rows { response; }
    }
}
concord::api! {
    pub client Lowercase {
        base "http://x.test";
        GET rows() -> Rows { response csv; }
    }
}
```

Excerpt of [`format_reserved_words.stderr`](../crates/concord/tests/ui/format_reserved_words.stderr):

```text
error: expected a format: `json`, `text`, `bytes` or `form`, or a type path (`request Ndjson;`)
 --> tests/ui/format_reserved_words.rs:7:38
  |
7 |         GET item() -> Rows { response; }
  |                                      ^

error: `csv` is not a format: the built-in formats are `json`, `text`, `bytes` or `form`; a format type is written as a path (`self::csv`)
  --> tests/ui/format_reserved_words.rs:13:39
   |
13 |         GET rows() -> Rows { response csv; }
   |                                       ^^^
```

**Fix:** a statement names a format: `json`, `text`, `bytes`, `form`, or a type path; a lowercase format type is written as a path (`self::csv`). In a parameter list a misspelled placement (`qurey`) is reported with the list of placements.

### A format that does not support the payload type

Excerpt of [`representation_unsupported_payload.rs`](../crates/concord/tests/ui/representation_unsupported_payload.rs):

```rust
pub struct Item;
concord::api! {
    pub client Api {
        base "http://x.test";
        GET item() -> Item { response text; }
        POST create(body item: Item) { request bytes; }
    }
}
concord::api! {
    pub client Scalar {
        base "http://x.test";
        POST count(body n: u64) { request text; }
        GET total() -> u64 { response bytes; }
    }
```

Excerpt of [`representation_unsupported_payload.stderr`](../crates/concord/tests/ui/representation_unsupported_payload.stderr):

```text
error[E0277]: the format `Text` cannot decode a `Item` response
 --> tests/ui/representation_unsupported_payload.rs:8:39
  |
8 |         GET item() -> Item { response text; }
  |                                       ^^^^ this output's format is `Text` (the nearest `response` statement: on the endpoint, a scope or the client); it needs `Text: Decode<Item>`
```

**Fix:** pick a format that supports the type (`json` for a serde type), or implement `Decode<Item>`/`Encode<Item>` for your own format type. An inherited format is reported at the statement that gave it (`format_inherited_unsupported.stderr`). For `json`, an unmet serde bound is reported by rustc in its own words (`the trait bound .. Deserialize<'de> is not satisfied`, `tests/ui_json/format_serde_bound.stderr`).

### An invalid media type in a format

Excerpt of [`format_invalid_content_type.rs`](../crates/concord/tests/ui/format_invalid_content_type.rs):

```rust
pub struct Odd;
impl concord::Encode<String> for Odd {
    const CONTENT_TYPE: Option<&'static str> = Some("application/x\u{2011}odd");
    fn encode(value: String) -> Result<concord::reqwest::Body, concord::BodyError> {
        Ok(value.into())
    }
}
concord::api! {
    pub client Api {
        base "http://x.test";
        POST odd(body b: String) { request Odd; }
    }
}
fn main() {}
```

Excerpt of [`format_invalid_content_type.stderr`](../crates/concord/tests/ui/format_invalid_content_type.stderr):

```text
error[E0080]: evaluation panicked: HeaderValue::from_static with invalid bytes
  --> tests/ui/format_invalid_content_type.rs:13:44
   |
13 |         POST odd(body b: String) { request Odd; }
   |                                            ^^^ evaluation of `ApiOdd::prepare::__FORMAT` failed inside this call
```

**Fix:** `Encode::CONTENT_TYPE` must be a valid static header value (visible ASCII): it is evaluated when the declaration is compiled, wherever it may be sent. A media type declared on a `request` statement replaces it and is checked by the macro instead, with the `mime` crate (`format_media_type_invalid.stderr`); a media type with no format is `format_media_type_needs_format.stderr`.

## Query and header composition

### An optional override without `else inherit`

Excerpt of [`override_needs_else_inherit.rs`](../crates/concord/tests/ui/override_needs_else_inherit.rs):

```rust
concord::api! {
    pub client Api(query locale: String = "en") {
        base "http://x.test";
        GET page(page_locale?: String) { query { override "locale" = page_locale; } }
    }
}
```

Excerpt of [`override_needs_else_inherit.stderr`](../crates/concord/tests/ui/override_needs_else_inherit.stderr):

```text
error: this `override` reads an optional value, so it can be absent; say what absence means: `override "locale" = ... else inherit;` keeps the inherited value, `remove "locale"; "locale" = ...;` sends nothing
 --> tests/ui/override_needs_else_inherit.rs:5:50
  |
5 |         GET page(page_locale?: String) { query { override "locale" = page_locale; } }
  |                                                  ^^^^^^^^
```

**Fix:** choose: `override "locale" = page_locale else inherit;` keeps the client's locale when
`page_locale` is absent; `remove "locale";` plus `"locale" = page_locale;` sends none.

### `else inherit` on a value that is always present

Excerpt of [`else_inherit_needs_optional_source.stderr`](../crates/concord/tests/ui/else_inherit_needs_optional_source.stderr):

```text
error: `else inherit` never applies: this source is always present
 --> tests/ui/else_inherit_needs_optional_source.rs:5:49
  |
5 |         GET page(page_locale: String) { query { override "locale" = page_locale else inherit; } }
  |                                                 ^^^^^^^^
```

**Fix:** delete `else inherit`, or make the value optional (`page_locale?: String`).

### Setting an inherited key again

Excerpt of [`duplicate_inherited_key.stderr`](../crates/concord/tests/ui/duplicate_inherited_key.stderr):

```text
error: query key `page` is already set by `Api`; use `override` or `append` to change an inherited query key, or add `remove "page";` to this owner's block to replace it exactly
 --> tests/ui/duplicate_inherited_key.rs:6:43
  |
6 |             GET list(page: u32) { query { "page" = page; } }
  |                                           ^^^^^^
```

**Fix:** say how this owner changes the inherited key: `override`, `append`, or `remove` then
set it. Overriding or removing a key that nothing above sets is an error too
([`override_without_target.stderr`](../crates/concord/tests/ui/override_without_target.stderr),
[`remove_without_target.stderr`](../crates/concord/tests/ui/remove_without_target.stderr)).

When a header key in the chain is a native constant, the macro cannot see which header it holds;
a header set twice under two spellings it cannot prove equal is then a build error from
`prepare()`, on every call of that endpoint (names are compared before values, so also when the
second value is absent). From
[`native_keys.rs`](../crates/concord/tests/native_keys.rs):

```text
GET Keys.duplicate: request not built: `authorization` on `Keys.duplicate` sets a header that `AUTHORIZATION` on `Keys` already sets (the same header name, written differently); use `override` or `append`, or `remove` it first
```

**Fix:** the same as above. A test that prepares every endpoint finds these before a caller does.

## Generated names

### Two methods with one name

Excerpt of [`method_collisions.rs`](../crates/concord/tests/ui/method_collisions.rs):

```rust
concord::api! {
    pub client Api(lang?: String) {
        base "http://x.test";
        query { "lang" = lang; }
        GET set_lang();
        GET new();
    }
}
```

Excerpt of [`method_collisions.stderr`](../crates/concord/tests/ui/method_collisions.stderr):

```text
error: `Api` would have two methods named `set_lang`: the navigation method for `Api.set_lang` and the control for value `lang`; rename one of them
 --> tests/ui/method_collisions.rs:5:13
  |
5 |         GET set_lang();
  |             ^^^^^^^^

error: the control for value `lang` is declared here
 --> tests/ui/method_collisions.rs:2:20
  |
2 |     pub client Api(lang?: String) {
  |                    ^^^^

error: `Api` would have two methods named `new`: the navigation method for `Api.new` and the constructor; rename one of them
 --> tests/ui/method_collisions.rs:6:13
  |
6 |         GET new();
  |             ^^^
```

**Fix:** rename the endpoint, scope or value. Built-ins are `new`, `with_base_url`, `send`,
`prepare` and `clone` ([`builtin_collisions.stderr`](../crates/concord/tests/ui/builtin_collisions.stderr)).

### Two generated types with one name

A scope named `config` under client `Api` would be called `ApiConfig`, the name of the client's
config struct. Excerpt of [`config_name_collision.stderr`](../crates/concord/tests/ui/config_name_collision.stderr):

```text
error: generated type name `ApiConfig` is also the struct of the client's required values; rename this owner
 --> tests/ui/config_name_collision.rs:7:15
  |
7 |         scope config() {
  |               ^^^^^^

error: the client is declared here
 --> tests/ui/config_name_collision.rs:4:16
  |
4 |     pub client Api(key: String) {
  |                ^^^
```

**Fix:** rename the scope. Collisions *between* two `api!` invocations in one module are reported
by rustc as `E0428` ("the name ... is defined multiple times"); give each client its own module.

## Attributes

### An unsupported attribute

Excerpt of [`unknown_attribute.stderr`](../crates/concord/tests/ui/unknown_attribute.stderr):

```text
error: unsupported attribute; values accept documentation comments and `#[sensitive]`
 --> tests/ui/unknown_attribute.rs:4:17
  |
4 |         GET one(#[serde(rename = "x")] query x: String);
  |                 ^
```

**Fix:** values accept `///` and `#[sensitive]`; scopes and endpoints accept `///`, `#[cfg(..)]`
and `#[deprecated(..)]`; the client accepts `///`. On a scope or endpoint the message is
"unsupported attribute on a scope or endpoint; documentation comments, `#[cfg(..)]` and
`#[deprecated]` are accepted"
([`endpoint_attribute_unsupported.stderr`](../crates/concord/tests/ui/endpoint_attribute_unsupported.stderr)).

### A deprecated endpoint

`#[deprecated]` on an endpoint or scope is applied to its navigation method, so callers see the
standard lint where they navigate. Excerpt of
[`deprecated_endpoint.stderr`](../crates/concord/tests/ui/deprecated_endpoint.stderr) (compiler
lint, an error here because the test denies `deprecated`):

```text
error: use of deprecated method `Api::list`: use `items`
  --> tests/ui/deprecated_endpoint.rs:14:17
   |
14 |     let _ = api.list();
   |                 ^^^^
```

## All snapshots

| Area | UI tests (`crates/concord/tests/ui/`) |
|---|---|
| Structure | `missing_base`, `unknown_method`, `duplicate_statement`, `statement_after_child`, `body_errors`, `sensitive_body`, `unknown_attribute`, `endpoint_attribute_unsupported` |
| Names | `unknown_value`, `ancestor_needs_qualification`, `qualified_non_ancestor`, `self_qualified`, `template_raw_keyword`, `duplicate_value`, `unused_value`, `reserved_value_names`, `duplicate_names`, `method_collisions`, `builtin_collisions`, `config_name_collision` |
| Values and calls | `config_missing_field`, `missing_required_argument`, `newtype_is_not_converted`, `conversions_are_closed`, `only_optional_values_have_maybe`, `default_cannot_capture_arguments`, `endpoint_clone_needs_clone_values`, `deprecated_endpoint` |
| URL | `base_errors`, `base_invalid_port`, `path_errors`, `optional_in_path`, `scope_trailing_slash`, `subdomain_errors` |
| Query and headers | `invalid_header_name`, `header_literal_value`, `header_name_exceeds_native_limit`, `reserved_header`, `reserved_headers_all`, `duplicate_header_case`, `duplicate_inherited_key`, `override_without_target`, `override_needs_else_inherit`, `else_inherit_needs_optional_source`, `override_after_remove`, `remove_without_target`, `remove_acts_on_inherited`, `each_and_scalar` |
| Wire formatting | `option_has_no_wire_spelling`, `vec_needs_each` |
| Formats | `representation_missing_via`, `representation_unsupported_payload`, `representation_alpha_spelling`, `format_via_removed`, `format_endpoint_statement`, `format_output_media_type`, `format_header_media_conflict`, `format_media_type_invalid`, `format_invalid_content_type`, `format_inherited_unsupported`, `format_reserved_words`, `format_swapped_order`; with `json`: `ui_json/format_serde_bound`; with `json` and `form` off: `ui_features/feature_disabled_formats` |

Known rough edges (compiler-origin): an over-long literal header name is rejected by the native
validator as `E0080` ("evaluation panicked: HeaderName::from_static with invalid bytes") spanning
the whole invocation, and so is a native constant whose `HeaderName::from_static` name is not
lowercase (at the constant); an invalid `Encode::CONTENT_TYPE` is the same `E0080` for `HeaderValue`,
pointing at the format. A format word whose feature is off is rustc's "cannot find type `Json` in
crate `$crate`" at the word (one error per endpoint statement that names it, one per client or
scope statement that endpoints use) with its note "the item is gated behind the `json` feature"
(`ui_features/feature_disabled_formats.stderr`). Old-style `-> Json<Item>`, `Text`, `Binary` and
`-> ()` get a precise replacement (`representation_alpha_spelling`).
