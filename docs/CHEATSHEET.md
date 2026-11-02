# Concord cheat sheet

The whole language on one page. Each row links to its precise rule in the
[author guide](AUTHOR_GUIDE.md); the caller side is in the [caller guide](CALLER_GUIDE.md). The
example below is compiled and run as a doctest.

```rust
use concord::reqwest::{self, header::{ACCEPT, AUTHORIZATION, HeaderName}};
use serde::{Deserialize, Serialize};

/// A vendor header, as a native constant.
pub const X_TENANT: HeaderName = HeaderName::from_static("x-tenant");

#[derive(Deserialize)]
pub struct Doc { pub id: u64 }
#[derive(Serialize)]
pub struct NewDoc { pub title: String }
#[derive(Serialize)]
pub struct Rename { pub title: String }

concord::api! {
    /// A documents API.
    pub client Docs(
        #[sensitive] header(AUTHORIZATION) token: String, // required: a field of `DocsConfig`
        header(X_TENANT) tenant?: String,                 // optional: absent until set
    ) {
        base "https://api.docs.example/v1";
        headers { ACCEPT = "application/json"; "x-client" = "concord"; }
        format json; // requests and responses below are JSON unless a nearer statement says otherwise

        scope folder(id: u32) {
            path "folders/{id}";

            GET list(query page?: u32, query("per-page") per_page: u32 = 50) -> Vec<Doc> {
                path "docs";
            }
            POST create(body doc: NewDoc) -> Doc { path "docs"; }
            PATCH rename(doc: u64, body change: Rename) -> Doc {
                path "docs/{doc}";
                request json "application/merge-patch+json"; // format and media type together
            }
            PUT write(doc: u64, body markdown: String) {
                path "docs/{doc}/content";
                request text "text/markdown";
            }
            GET raw(doc: u64) -> Vec<u8> {
                path "docs/{doc}/raw";
                response bytes;
            }
            DELETE remove(doc: u64) { path "docs/{doc}"; } // no `->`: any 2xx, body not decoded
            GET public(doc: u64) -> Doc {
                path "public/{doc}";
                headers { remove AUTHORIZATION; override "x-client" = "concord-public"; }
            }
        }
    }
}

fn main() -> Result<(), concord::Error> {
    let config = DocsConfig { token: "Bearer t".into() };
    let docs = Docs::new(reqwest::Client::new(), config).set_tenant("acme");
    let folder = docs.folder(7);

    let list = folder.list().set_page(2).prepare()?;
    assert_eq!(
        list.request().url().as_str(),
        "https://api.docs.example/v1/folders/7/docs?page=2&per-page=50"
    );
    assert_eq!(list.request().headers()["x-tenant"], "acme");

    let rename = folder.rename(3, Rename { title: "Notes".into() }).prepare()?;
    assert_eq!(rename.request().headers()["content-type"], "application/merge-patch+json");

    let public = folder.public(3).prepare()?;
    assert!(public.request().headers().get("authorization").is_none());
    assert!(!format!("{docs:?}").contains("Bearer t")); // never printed by Concord
    Ok(())
}
```

## Declaring

| Write | Means | Rule |
|---|---|---|
| `pub client Name(..) { base ".."; .. }` | one API; its required values form `NameConfig` | [§1](AUTHOR_GUIDE.md#1-model-the-api-as-a-tree) |
| `scope name(..) { .. }` | a level the caller navigates: shared path, subdomain, keys, values | [§1](AUTHOR_GUIDE.md#1-model-the-api-as-a-tree) |
| `GET name(..) -> T { .. }` | an endpoint (`GET POST PUT PATCH DELETE HEAD OPTIONS`); no `->`: no typed output | [§5.5](AUTHOR_GUIDE.md#55-no-output-versus---) |
| `x: T`, `x: T = e`, `x?: T`, `x?: T = e` | required, initialized, optional, optional and initially present | [§2](AUTHOR_GUIDE.md#2-values-required-optional-initialized) |
| `path "a/{x}"`, `subdomain "{x}"` | templates; `{client.x}` or `{scope.x}` reads an enclosing owner's value | [§3](AUTHOR_GUIDE.md#3-base-paths-and-subdomains) |
| `query x`, `query("k") x`, `header("k") x`, `header(KEY) x`, `body x` | placements in the parameter list; `each` sends one occurrence per item | [§4.1](AUTHOR_GUIDE.md#41-placing-a-value) |
| `query { "k" = v; }`, `headers { "k" = "t {x}"; KEY = v; }` | blocks. **Left of `=` is a key**: a literal, or for headers a Rust path to a `HeaderName`. **Right of `=` is a value**: a template, or a value's name | [§4.2](AUTHOR_GUIDE.md#42-blocks-templates-and-references), [§4.8](AUTHOR_GUIDE.md#48-header-names-and-values) |
| `append`, `override`, `override .. else inherit`, `remove` | change a key set further up; anything else on an inherited key is an error | [§4.3](AUTHOR_GUIDE.md#43-what-an-owner-may-do-with-an-inherited-key) |
| `format F;`, `request F;`, `request F "type/subtype";`, `response F;` | formats on a client, scope or endpoint; the nearest wins per direction; a media type comes with its format | [§5.2](AUTHOR_GUIDE.md#52-where-formats-come-from) |
| `json`, `text`, `bytes`, `form`, or a path (`self::Csv`) | built-in formats, or a type implementing `Encode<T>`/`Decode<T>` | [§5.1](AUTHOR_GUIDE.md#51-the-formats), [§5.7](AUTHOR_GUIDE.md#57-custom-formats) |
| `#[sensitive]` | never printed by Concord; also every key, segment and label it feeds | [§7](AUTHOR_GUIDE.md#7-sensitive-values) |
| `#[cfg(..)]`, `#[deprecated]`, `///` on scopes and endpoints | applied to the subtree or the navigation method | [§8](AUTHOR_GUIDE.md#8-attributes) |

Usual order inside an owner: `base` (client), `path`, `subdomain`, `query { }`, `headers { }`,
format statements, then children.

## Calling

| Generated | Use |
|---|---|
| `Docs::new(http, DocsConfig { .. })` | builds the client; initializers run once here |
| `.set_x(v)`, `.maybe_x(Some(v) / None)` | return the updated handle (`#[must_use]`); handles are cheap snapshots |
| `docs.folder(7)`, `folder.rename(3, change)` | navigation: required values are positional, in declaration order |
| `.send().await` | `prepare`, one `execute`, require 2xx, decode; never retries |
| `.prepare()?` → `Prepared` | the native `reqwest::Request` (`request()`), `send()`, `send_raw()`, `into_parts()` |
| `into_parts()` → `(Client, Request, Decoder)` | execute it yourself, then `decoder.finish(result)` or `decoder.decode(response)` |
| `error.kind()`, `status()`, `endpoint()` | `Build`, `Transport`, `Status`, `Decode` |
| `error.response()`, `error.into_response()` | a status error's unread response: borrowed (headers) or taken (body) |
| `error.transport()`, `error.body_source()` | the underlying errors, which may contain URLs or body content |

## Remember

- The format, not the Rust type, decides the wire: `-> String` under `json` decodes a JSON string;
  `request text;` sends raw text. An empty response is not JSON: an endpoint that returns nothing
  omits `->`.
- An optional body: `set_b(v)` always sends `v` encoded (`None` in an `Option<T>` body is `null`);
  `maybe_b(None)` sends no body.
- Initializers run each time their owner is created: put a per-request value (an idempotency key)
  on the endpoint, not the client.
- Handles are snapshots: after `with_base_url` or a setter on the client, create scopes again from
  the new handle.
- Concord sends no `Accept` and never checks the response `Content-Type`; reqwest adds `accept:
  */*`, `content-length` and `host` when it executes the request.
- The list of well-known secret names is a safety net; mark every secret `#[sensitive]`.
