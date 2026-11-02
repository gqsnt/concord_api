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
  representations and values carry Concord's own text and notes) but their layout, wording and
  sometimes their number depend on the compiler version. The snapshots are pinned to rustc 1.97.1
  (`rust-toolchain.toml`); other versions print variations of them.

None of these messages contains a runtime value: compile-time errors only quote the declaration.

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
error: `idd` is not a value of `Api.item`
 --> tests/ui/unknown_value.rs:4:37
  |
4 |         GET item(id: String) { path "items/{idd}"; query { "id" = id; } }
  |                                     ^^^^^^^^^^^^^
```

**Fix:** correct the name, or declare the value on this owner.

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

Arguments and setters accept exactly the declared type, or `&str`/`&String` for `String`.
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
   = note: Concord accepts the exact declared type, or `&str`/`&String` for `String`; construct newtypes and `Option`s explicitly
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
error: "x tag" is not a valid header field name
 --> tests/ui/invalid_header_name.rs:4:19
  |
4 |         headers { "x tag" = "a"; }
  |                   ^^^^^^^
```

Excerpt of [`header_literal_value.stderr`](../crates/concord/tests/ui/header_literal_value.stderr):

```text
error: 'é' is not allowed in a header value; header values are visible ASCII, space or tab
 --> tests/ui/header_literal_value.rs:4:40
  |
4 |         GET e() { headers { "x-name" = "café"; } }
  |                                        ^^^^^^
```

**Fix:** header names are tokens (letters, digits and ``!#$%&'*+-.^_`|~``); values are visible
ASCII, space or tab. Transport-owned names such as `host` or `content-length` are rejected too
([`reserved_headers_all.stderr`](../crates/concord/tests/ui/reserved_headers_all.stderr)). A value
formatted at runtime that breaks the rule is a build error from `prepare()`, naming the header.

### A type without `Display`

Placed values are written with `Display`. Excerpt of
[`option_has_no_wire_spelling.rs`](../crates/concord/tests/ui/option_has_no_wire_spelling.rs):

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
error[E0277]: `Option<String>` doesn't implement `std::fmt::Display`
 --> tests/ui/option_has_no_wire_spelling.rs:4:32
  |
4 |         GET list(query filter: Option<String>);
  |                                ^^^^^^ the trait `std::fmt::Display` is not implemented for `Option<String>`
```

**Fix:** for "maybe absent", declare the value optional (`query filter?: String`); for a list, use
`each` ([`vec_needs_each.rs`](../crates/concord/tests/ui/vec_needs_each.rs)); for another spelling,
a newtype with the `Display` you need.

## Bodies and responses

### A missing `via`

Excerpt of [`representation_missing_via.rs`](../crates/concord/tests/ui/representation_missing_via.rs):

```rust
pub struct Item;
concord::api! {
    pub client Api {
        base "http://x.test";
        GET item() -> Item;
    }
}
```

Excerpt of [`representation_missing_via.stderr`](../crates/concord/tests/ui/representation_missing_via.stderr):

```text
error: the response representation is explicit: write `Item via Json` (or `via Text`, `via Binary`, or your own representation type)
 --> tests/ui/representation_missing_via.rs:6:23
  |
6 |         GET item() -> Item;
  |                       ^^^^
```

**Fix:** name the representation. An endpoint with no typed output has no `->` at all.

### A representation that does not support the payload type

Excerpt of [`representation_unsupported_payload.rs`](../crates/concord/tests/ui/representation_unsupported_payload.rs):

```rust
pub struct Item;
concord::api! {
    pub client Api {
        base "http://x.test";
        GET item() -> Item via concord::Text;
        POST create(body item: Item via concord::Binary);
    }
}
```

Excerpt of [`representation_unsupported_payload.stderr`](../crates/concord/tests/ui/representation_unsupported_payload.stderr)
(compiler message with Concord's text and note):

```text
error[E0277]: `Text` cannot decode a `Item` response
 --> tests/ui/representation_unsupported_payload.rs:7:32
  |
7 |         GET item() -> Item via concord::Text;
  |                                ^^^^^^^^^^^^^ `-> Item via Text` needs `Text: Decode<Item>`
  |
  = note: built in: `Json` (any `DeserializeOwned`, feature `json`), `Text` (`String`), `Binary` (`Bytes`, `Vec<u8>`); or implement `concord::Decode<Item>` for your own marker type
```

**Fix:** pick a representation that supports the type (`Item via Json` for a serde type), or
implement `Decode<Item>`/`Encode<Item>` for your own marker type.

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
error: generated type name `ApiConfig` is used twice
 --> tests/ui/config_name_collision.rs:7:15
  |
7 |         scope config() {
  |               ^^^^^^

error: also generated here
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
and `#[deprecated(..)]`; the client accepts `///`. On a scope or endpoint, the message currently
reads "unsupported attribute here; only documentation comments are allowed" although `#[cfg]` and
`#[deprecated]` are accepted there; that message is not covered by a snapshot yet.

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
| Structure | `missing_base`, `unknown_method`, `duplicate_statement`, `statement_after_child`, `body_errors`, `sensitive_body`, `unknown_attribute` |
| Names | `unknown_value`, `ancestor_needs_qualification`, `qualified_non_ancestor`, `self_qualified`, `template_raw_keyword`, `duplicate_value`, `unused_value`, `reserved_value_names`, `duplicate_names`, `method_collisions`, `builtin_collisions`, `config_name_collision` |
| Values and calls | `config_missing_field`, `missing_required_argument`, `newtype_is_not_converted`, `conversions_are_closed`, `only_optional_values_have_maybe`, `default_cannot_capture_arguments`, `endpoint_clone_needs_clone_values`, `deprecated_endpoint` |
| URL | `base_errors`, `base_invalid_port`, `path_errors`, `optional_in_path`, `scope_trailing_slash`, `subdomain_errors` |
| Query and headers | `invalid_header_name`, `header_literal_value`, `header_name_exceeds_native_limit`, `reserved_header`, `reserved_headers_all`, `duplicate_header_case`, `duplicate_inherited_key`, `override_without_target`, `override_needs_else_inherit`, `else_inherit_needs_optional_source`, `override_after_remove`, `remove_without_target`, `remove_acts_on_inherited`, `each_and_scalar` |
| Wire formatting | `option_has_no_wire_spelling`, `vec_needs_each` |
| Representations | `representation_missing_via`, `representation_unsupported_payload` |

Known rough edges (compiler-origin or not yet improved): an over-long literal header name is
rejected by the native validator as `E0080` ("evaluation panicked: HeaderName::from_static with
invalid bytes") spanning the whole invocation; a missing `via` on an old-style `-> Json<Item>`
suggests `Json < Item > via Json` rather than `Item via Json`.
