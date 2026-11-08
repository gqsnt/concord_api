# Migration

Eight paths, newest first: the interface consolidation (unreleased candidate), 4.1 to 4.2 (a
maintenance release; summary in the [changelog](../CHANGELOG.md)), then the two parts of 4.1.0
(the modular V4), then older ones:

- [To the consolidated interface](#to-the-consolidated-interface) — additive: existing code
  compiles (one rare exception: a generated name, below); the new response, body, stream and
  pagination boundaries, declared limits and send results, parameter groups and `query(..)` values
  replace SDK plumbing, and the standard formats moved to `concord-codecs`.
- [From 4.1 to 4.2](#from-41-to-42) — same DSL, crates, codec contracts and toolchain; some
  declarations that 4.1 rejected compile, advice is exact, and one rare `macro_rules!` case changes
  meaning.
- [From format words to format types](#from-format-words-to-format-types) — the modular V4
  (`concord-v4-modular`), coming from `concord-v4-final`. Manual, guided by the compiler; callers
  need no change.
- [From body-only codecs to the codec contract](#from-body-only-codecs-to-the-codec-contract) —
  the same release. A custom `Decode` is rewritten, `BodyError` becomes `CodecError`, and callers
  can meet new errors: a response is checked before it is parsed, and typed JSON endpoints send
  an `Accept`.
- [From in-place formats to statements](#from-in-place-formats-to-statements) — the body and
  response syntax and the header names of the final V4 (`concord-v4-final`), coming from
  `concord-v4-native-repr`. Manual, guided by the compiler; callers need no change.
- [From `via`](#from-via-to-statements) — the body and response syntax of the refined V4 up to
  commit `c112845`.
- [From the V4 alpha to the refined V4](#from-the-v4-alpha-400-alpha1-at-8f7adc2-to-the-refined-v4)
  — a set of targeted syntax and API changes to the same design.
- [From V3 to V4](#from-concord-v3-to-v4) — a redesign; declarations are rewritten by hand.

Code blocks marked "does not compile" show the old form. Every other Rust block in this file is
compiled and run as a doctest of `examples/quickstart` (`cargo test --doc -p
concord-example-quickstart`).

## To the consolidated interface

Existing runtime entry points remain: `send`, `send_raw`, `into_parts`, `Decoder::{decode, finish}`,
`decode_limited`, `bytes_limited`, `Error::meta()`, the reader `open`/`open_for` methods, the
pagination engine and the `concord-extras` crate work as before. The paragraphs below say which SDK
code can shrink and, where adopting a new form changes what an SDK's own callers observe, what
changes for them.

**H06 example SDK controls.** The opt-in `controls { pub(crate) name; remove name; }`
statement changes only the selected SDK controls. Unlisted controls and all non-adopting SDKs
keep their API. These example SDK changes are breaking for callers of a withheld control;
required-value setters listed below have no `maybe_x` counterpart. Existing valid requests keep
their wire behavior; QuickBooks also selects the replacement connection's environment at send
to preserve coordinated replacement across environments. `prepare()` and native request access
remain the documented escape boundary.

- **adyen-checkout-sdk:** `Checkout::set_api_key`, `set_policy` removed. Construct `Checkout::sandbox(new_key, shared_company_policy.clone())` or `Checkout::new(http, CheckoutConfig { api_key, policy })`. Preserve the ledger when rotating keys for the same company; another company uses its own shared policy.
- **datadog-events-sdk:** `DatadogApi::new` crate-visible; `set_api_key`, `set_application_key` removed. Use `DatadogEvents::new(http, site, Credentials::new(api_key, application_key)?)` and `api()`. Replace keys by constructing another facade. Proxy/fixture `with_base_url` remains public; `site()` retains the configured site.
- **digitalocean-sdk:** `DigitalOcean::with_base_url` crate-visible. Use `DigitalOcean::connect(token)` for production or `DigitalOcean::for_fixture(token, origin)` for a bare HTTP literal-loopback origin. Native `new` at production and credential setters remain public.
- **discord-rest-sdk:** `Discord::set_token`, `set_limiter` removed. Construct `Discord::new(http, DiscordConfig { token, limiter })` with that bot token and its shared limiter together. Base replacement and other setters remain public.
- **mastodon-sdk:** Pilot: `Api::new`, `with_base_url` crate-visible. Use `Mastodon::new(http, instance)` and `api()`; rotate through `with_token`. Generated Api remains publicly nameable; advertised streaming-host validation stays in the facade.
- **quickbooks-online-sdk:** `QuickBooks::new`, `with_base_url` crate-visible. Use `QuickBooks::connect(http, connection)` so the connection selects sandbox or production, or `QuickBooks::for_fixture(http, connection, origin)` for bare HTTP on a literal loopback IP. `set_connection` still replaces realm, credential source and policy together. The send policy selects the replacement connection's official environment host (loopback fixtures keep their origin); token-free `prepare()` shows the base snapshot. Refresh-token replacement is unchanged.
- **salesforce-sdk:** `SalesforceApi::new`, `with_base_url` crate-visible; pilot `set_version` removed. Use `OrgContext::new(http, instance, version, token)` with `version >= ApiVersion::MINIMUM`, then `api()`. Another instance or version requires another validated context.
- **sentry-sdk:** `Sentry::with_base_url` crate-visible; `set_authorization`, `set_policy` removed. Use `Sentry::with_token(http, token, policy)` or `Sentry::for_deployment(http, base, token, policy)` for self-hosting/fixtures. Rotate same-caller tokens with a clone of the same caller/deployment policy; different callers/deployments need their own policies. Native `new` still accepts the complete header and policy together.
- **shopify-sdk:** Pilot: `ShopifyAdminShop::set_shop` removed. Enter `shop(other_shop, token_for_other_shop)` again, or construct the other ShopContext. Same-shop `set_access_token` rotation stays public.
- **usgs-earthquake-catalog-sdk:** `Usgs::new`, `with_base_url` crate-visible. Use `Usgs::for_fixture(http, origin)` with a bare HTTP origin on a literal loopback IP, without `/fdsnws/event/1`. The caller still configures transport timeouts and disables redirects/retries.
- **zendesk-support-sdk:** `Zendesk::with_base_url` crate-visible; pilot `set_subdomain` removed. Use `Zendesk::connect(http, subdomain, token)` for another account. Fixtures use `Zendesk::for_fixture(http_builder, subdomain, token, loopback_socket_address)`: it resolves `{subdomain}.zendesk.test` to that address with proxies disabled. Keep caller retry/redirect/timeout settings in the builder. `set_policy` and same-account `set_authorization` rotation remain public.

**One new generated name.** Every generated endpoint type now has an inherent
`operation() -> concord::Operation` (its declared identity, without naming the output). Like
`prepare` and the `set_*` controls, the name is the generated type's: an SDK that added its own
inherent `operation` (or an item of that name) to a generated endpoint type gets `E0592` and
deletes it — the generated one returns the same identity as `<E as Endpoint<T>>::operation()`.
A `use concord::Endpoint;` that only served `GeneratedType::operation()` becomes unused (a
warning; an error under `-D warnings`) and can be removed.

**Reading response metadata.** Code that took the request apart only to look at the head before
decoding (a fragment of the old form; it does not compile on its own):

```rust,ignore
// Before (still compiles; does not need to be written any more).
let (client, request, decoder) = prepared.into_parts();
let response = match client.execute(request).await {
    Ok(response) => response,
    Err(e) => {
        let Err(error) = decoder.finish(Err(e)).await else { unreachable!() };
        return Err(error);
    }
};
let request_id = response.headers().get("x-request-id").cloned();
let value = decoder.decode(response).await?;
```

becomes one execution with the head in between:

```rust
# use concord::Json;
# #[derive(serde::Deserialize)] pub struct Summary { pub total: u64 }
# concord::api! { pub client Ledger { base "https://ledger.example.test"; format Json;
#     GET summary(id: u64) -> Summary { path "summary/{id}"; } } }
async fn summary(api: &Ledger, id: u64) -> Result<(Summary, Option<String>), concord::Error> {
    let received = api.summary(id).prepare()?.receive().await?;
    let request_id = received
        .head()
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    Ok((received.decode().await?, request_id))
}
```

A request you edit natively (a `Link` URL) goes through `Decoder::execute(&client, request)`,
which returns the same `Received`; a native transport failure converts with
`decoder.transport_error(e)`.

**Metadata beside a value or an error.** A policy that cloned the head before decoding, to pair it
with the value or with the error (a fragment of the old form; it does not compile on its own):

```rust,ignore
// Before (still compiles; does not need to be written any more).
let meta = received.meta();
match received.decode().await {
    Ok(value) => Ok((value, meta)),
    Err(error) => Err(SdkError { error, meta: Some(meta) }),
}
```

takes the value with its head from `Received::decode_with_meta()` (or
`decode_limited_with_meta(n)`), and the head of a failure from the error:

```rust
# use concord::Json;
# #[derive(serde::Deserialize)] pub struct Summary { pub total: u64 }
# concord::api! { pub client Ledger { base "https://ledger.example.test"; format Json;
#     GET summary(id: u64) -> Summary { path "summary/{id}"; } } }
use concord::ResponseMeta;

/// The SDK's error keeps the response head of any failure after one arrived.
pub struct SdkError {
    pub error: concord::Error,
    pub meta: Option<ResponseMeta>,
}

impl From<concord::Error> for SdkError {
    fn from(error: concord::Error) -> Self {
        let meta = error.head_meta(); // `None` only when no response head arrived
        Self { error, meta }
    }
}

async fn summary(api: &Ledger, id: u64) -> Result<(Summary, ResponseMeta), SdkError> {
    let received = api.summary(id).prepare()?.receive().await?;
    Ok(received.decode_with_meta().await?)
}
```

`Error::head()` (borrowed) and `Error::head_meta()` (owned) are `Some` for every error raised after
a response head arrived: `Status`, `Metadata`, and a `Decode`, `Limit` or `Transport` error while
the body was read. `Error::meta()` keeps its contract — `Some` only for a `Status` or `Metadata`
error, exactly when `into_response()` returns the response — so code that reads `meta()` or
`status()` keeps its meaning; the status of a decode failure is `error.head().map(|h| h.status())`.

**Provider error documents.** A policy that read an error document with
`received.bytes_limited(n)` and then `<Json as Decode<D>>::decode_body((), bytes)` skipped the
format's head check and `Context`. `received.document::<Json, D>(n).await` runs both before
reading, reads at most `n` bytes and returns `(ResponseMeta, Result<D, Error>)` for any status: the
head is kept whatever happened to the document, and a body that does not decode stays readable
through `error.body_unredacted()`. What the SDK's callers can notice: a document whose
`Content-Type` the format refuses is now a `Metadata` error instead of being parsed. For a provider
that labels its errors loosely, read it with `document::<IgnoreContentType<Json>, D>`, as the
[quick start](QUICKSTART.md#4-configure-the-client-call-handle-the-result) does.

**Bounds declared once.** A policy that called `received.decode_limited(n)` with a fixed `n`
declares `limit N;` on the client, a scope or the endpoint (the nearest wins) and calls
`received.decode()` (or `decode_with_meta()`). The bound travels with the endpoint's handling, so
the generated `send()` and a caller's own `prepare()?.send()` apply it too: a caller that read an
unbounded body through `prepare()` now gets `ErrorKind::Limit` beyond `N`. A bound chosen at run
time stays `decode_limited(n)`, which replaces the declared one for that call. `limit` on an
endpoint without `->` or with `-> stream` does not compile
([author guide](AUTHOR_GUIDE.md#510a-bounding-finite-bodies-limit)).

**Header fields.** A counter parsed with `to_str()` and `parse::<u64>()` (which accepts `+5`), or a
request id read with `headers().get(name)` (the first of several lines), becomes
`concord::field::unsigned(headers, name)` or `field::single(headers, name)` on any `HeaderMap`
(`ResponseMeta::headers()`, a native response's), or the same methods of a `ResponseHead`
(`received.head().unsigned(name)`, `ResponseHead::single`, also on `error.head()`). Both return
`Ok(None)` for an absent field and a `concord::FieldError` (`Repeated`, `Invalid`, `Overflow`) that
never quotes the value. An SDK that switches stops accepting a signed or repeated value its own
parser took, which its callers can notice.

**Failed writes.** An SDK that classified ambiguous writes by hand reads `Error::delivery()`
(`concord::Delivery`): `NotSent` when Concord refused the request while building it
(`ErrorKind::Build`), `Answered` once a response head arrived (whatever then happened to the body),
`Unknown` otherwise. A rule "`kind() == ErrorKind::Build` means not sent" is
`delivery() == Delivery::NotSent`. A rule that also took a connect error
(`transport().is_some_and(|e| e.is_connect())`) as "not sent" is not what `delivery()` says: a
refused connection is `Unknown`, because the client may have followed a redirect from a server that
performed the write. An SDK that accepts its caller's `reqwest::Client` and moves to `delivery()`
therefore reports such a write as uncertain where it reported it as not sent, which its callers
observe; only an SDK that builds its own client with redirects and retries off may keep the connect
rule ([author guide](AUTHOR_GUIDE.md#511b-a-write-that-may-have-happened-errordelivery)).

**Streaming endpoints.** An endpoint declared without `->` drains its body; a watch or an export
declared that way and opened with `send_raw()` + `reader.open_for(response, "Name")` should
declare its stream instead: `-> stream T` with an incremental `response` format returns the reader
from `send()`, `-> stream` returns the unread `Incoming` body for a reader configured at run time
(`reader.open(incoming)`), and the reader names the endpoint without the string. See the
[author guide](AUTHOR_GUIDE.md#59-streaming-outputs---stream).

**Runtime-encoded bodies.** An upload declared without a body and given an `Upload` with
`upload.apply(&mut request)` after `into_parts()` declares its body instead:
`body export: Upload` with `request Encoded;` ([author guide](AUTHOR_GUIDE.md#510-bodies-whose-media-type-is-known-at-run-time)).
A custom format whose media type depends on the payload overrides `Encode::encode_body`. A
wrapper format must forward `encode_body` too.

**Pagination.** An SDK that returned a request factory so that callers could assemble
`sequential(..)` returns a `concord_pagination::Pages` (or `PageItems`) instead; a caller's
`while let Some(page) = pages.next().await` loop that ignored the finish report becomes
`while let Some(page) = pages.next_page().await?`, which fails on a cap or a provider error instead
of ending silently. `JobsReport::into_resume` replaces cloning `discovery.resume` before
`into_rerun`. A hand-written `Link` tokenizer becomes `concord_pagination::link::parse` and
`link::relation` (feature `link`); the parser is strict (the first `rel` wins; a bare `rel`, a
trailing `;` or an empty unquoted value is an error), so an SDK that adopts it may refuse a header
its tokenizer accepted. Offset arithmetic becomes `next_offset(offset, limit, len, total)`, and a
`match` on `PagesError::Provider` that only extracted the provider's error becomes
`provider_error()` or `into_provider_error()`. Item-oriented code that needs each page's metadata
can opt into `Pages::item_events()`.

**The result of `send()`.** A client with `send Policy;` can declare the shape of the result:
`send Policy -> Result<_, Error>;`, where `_` stands for each endpoint's output. The generated
`send()` then returns `impl Future<Output = Result<Item, Error>> + Send` and its rustdoc names that
type, where `send Policy;` shows `<Policy as SendWith<Item>>::Output`. The template must be the
policy's `Output` exactly (otherwise the declaration fails at the template), so callers' code
compiles and behaves as before: only the signature and documentation they read change. A policy
implemented for a few concrete outputs keeps `send Policy;`; a `_` inside a macro invocation
(`result!(_)`) is not a placeholder
([author guide](AUTHOR_GUIDE.md#513-the-sdks-own-send-send-policy)).

**Helpers that send through the policy.** SDK code written once over several endpoints (every page
of a listing) that had to be handed the policy value, or that called `prepare()?.send()` and so
bypassed the policy, bounds on `concord::Sends<O>` and calls `send()`: every endpoint of a client
with a send policy implements it, `O` being the policy's output for that endpoint. A client
without a policy has no `Sends`: its `send()` is `prepare()?.send()`, which generic code reaches
through `Endpoint<T>`.

**Parameter groups.** Optional or initialized values repeated on many endpoints (list selectors, an
"act as this account" header), or a hand-written trait that forwarded each endpoint's setters,
become one `params Name(..);` on the client and `..Name` in each endpoint's values. The endpoints
keep the same `set_x`/`maybe_x` setters and the values go on the wire where `..Name` is written, so
callers need no change when the group is included where the values were. The group also generates
a trait `Name` in the declaring module, implemented by every endpoint that includes it, for SDK
code over those endpoints: a forwarding trait can shrink to default methods bounded on it, or go.
Required values stay on each endpoint
([author guide](AUTHOR_GUIDE.md#21-parameter-groups-one-fact-many-endpoints)).

**Values that choose their keys.** An SDK that wrote one wrapper endpoint per exclusive cursor
(`before`, `after`, `around`), or edited the URL after `into_parts()` to add keys named at run time
(`param_<name>`), declares one value `query(..) x?: T` whose type implements `concord::QueryPairs`.
Its pairs are written where the value is declared among the endpoint's own parameters; it is an
endpoint value (a parameter group may hold it) and cannot be `#[sensitive]`, so a secret keeps its
own keyed placement. The SDK's callers then set one value instead of choosing among wrapper
methods, an API change the SDK makes deliberately
([author guide](AUTHOR_GUIDE.md#41a-values-that-choose-their-own-keys-query)).

**The codecs package.** The standard formats are in `concord-codecs` (features `json`, `form`,
`stream`, `csv`, `jsonl`, `multipart`, `sse`; none on by default). A crate that depends on
`concord` and its features changes nothing. A crate that depends on `concord-extras` keeps
compiling: it is now a compatibility facade that re-exports `concord_codecs::*` (the same items, so
values pass between both paths) and forwards the same feature names; new code depends on
`concord-codecs`. A custom format that parsed `Content-Type` itself can call
`concord_codecs::check_content_type(head, &["application/json"], Charset::Utf8)` from its
`check_content_type` (`concord` does not re-export it).

## From 4.1 to 4.2

Update the four crates together (`=4.2.0`). The statement DSL, the request types, `IntoValue`,
the formats and their metadata policies, `prepare()`'s order and the Rust 1.99.1 minimum are
unchanged. Declarations and call sites need no edit, except where an expression fragment from
your own `macro_rules!` changes meaning (second point). What changes:

- **Declarations that 4.1 rejected compile.** A type macro is an output type like any other
  (`-> payload!()`, `-> m::payload![]`, `-> ::c::payload! {}`), where 4.1 read it as the removed
  in-place format ("expected curly braces"). Two crates' macros that pass types written with the
  same tokens (`$crate::Payload`) no longer make a declaration fail with E0277 when only one of the
  types is `Clone`: the request type is simply not `Clone`, and cloning it is the error. A
  `dyn A + B` forwarded by `macro_rules!` behind `&` is `&(dyn A + B)`, as in Rust, not an
  ambiguous `+`.
- **Forwarded expressions mean what Rust says.** A declaration produced by your own
  `macro_rules!`, with an expression fragment inside a value's type or initializer
  (`Payload<{ $e * 2 }>`, `= $e * 2` with `$e = 1 + 2`), keeps the fragment's grouping: 6, where
  4.1 computed 5. If you relied on the 4.1 value, write the expression you mean explicitly.
- **Migration advice is exact.** The messages for the removed spellings (`via`, in-place and
  trailing formats) quote the payload type token for token: literals keep their contents
  (`"a :: b"`, not `"a::b"`), a raw name stays raw (`body r#type`), and a macro's input keeps its
  spacing (`choose!(. .)` is not `choose!(..)`). A payload holding a `macro_rules!` fragment is
  named `T` instead of spelled, since no written text is that fragment. Generated documentation
  shows types and initializers the same way.

## From format words to format types

A statement names its format by an ordinary Rust type path. The words `json`, `text`, `bytes` and
`form` are no longer special: the formats are the types `Json`, `Text`, `Binary` and `Form`, named
like any other type, with no privilege over a format of yours.

| Before | After |
|---|---|
| `format json;`, `request json;`, `response json;` | `use concord::Json;` and `format Json;` (`request Json;`, `response Json;`) |
| `request text;`, `response text;` | `Text` |
| `request bytes "image/png";`, `response bytes;` | `Binary` (`Bytes` is the payload type, not the format) |
| `request form;` | `Form` (feature `form`) |
| `request self::csv;` (a lowercase type written as a path) | `request csv;` also works now; any path works as before |

Each old word is now an unknown type: rustc reports ``cannot find type `json` in this scope`` at
the word. rustc gives no import suggestion there (the name is resolved inside the macro's
expansion), so add the import yourself, or write the path (`format concord::Json;`). A format named
with its feature off (`Json` without `json`, `Form` without `form`) fails at the import, and rustc
names the feature. `use concord::Json as json;` is ordinary Rust and keeps `format json;`
compiling, but the capitalized name is the convention. Generated signatures, payloads and calling
code are unchanged.

There is no automatic rewriter (the earlier regex script stays withdrawn): the compiler lists every
statement to change, and each change is one word.

## From body-only codecs to the codec contract

The same release as the format types above. `Decode` now reads the response head, a format's
failure is a `CodecError`, and the standard formats check a response before they parse it. The
compiler finds every place that names the old API. The changes in behaviour show up at run time,
on the first calls against a server or a test fixture.

| Before | After |
|---|---|
| `Decode<T>`: `fn decode(body: Bytes) -> Result<T, BodyError>` | `type Context`, `fn context(&ResponseHead) -> Result<Self::Context, CodecError>`, `fn decode_body(Self::Context, Bytes) -> Result<T, CodecError>`; optional `check_content_type` and `ACCEPT` |
| `Encode<T>`: `fn encode(T) -> Result<Body, BodyError>` | `fn encode(T) -> Result<Body, CodecError>` |
| `BodyError::new(impl Into<Cow<'static, str>>)`, `description()` | `CodecError::new(&'static str)`, `label()`; numbers with `.at("line", n)` |
| `BodyError::body_source()`, `Error::body_source()` | `CodecError::source_unredacted()`, `Error::codec_error()` |
| `<Json as Decode<T>>::decode(bytes)` | `<Json as Decode<T>>::decode_body((), bytes)` |
| `ErrorKind::{Build, Transport, Status, Decode}` | plus `ErrorKind::Metadata` |
| `Error::response()`, `into_response()`: the response of a `Status` error | the response of a `Status` **or** a `Metadata` error |
| `Json` ignores the response `Content-Type` | `Json` refuses a response without exactly one `application/json` or `application/*+json` `Content-Type` |
| Concord sends no `Accept` | a typed output sends its format's suggestion, unless you declare an `accept` |
| one crate | four crates; you still depend on `concord` only |
| "a test that prepares every endpoint" finds header-name mistakes of native keys | `Client::validate_header_names()` checks every endpoint without building anything ([author guide §4.9](AUTHOR_GUIDE.md#49-checking-header-names-in-a-test)) |
| `concord::__private::NativeKey` could be implemented downstream (hidden, unsupported) | sealed: a header key path must name a `HeaderName` itself |
| the minimum Rust was 1.85 | the minimum Rust is 1.99.1 |

### What you may notice when you run

The first two items change what a working program does; the others change what it reads or
matches.

**A response is refused before it is parsed.** A 2xx response whose head the endpoint's format
refuses is an `ErrorKind::Metadata` error, and its body is not read. `Json` refuses a response
that has no `Content-Type`, more than one (even equal), a comma-joined or malformed one, a media
range, a type other than `application/json` or `application/*+json`, or a `charset` other than
`utf-8` or `us-ascii` (or declared twice). `Form` is as strict about `application/x-www-form-urlencoded`. `Text` refuses only a
single well-formed `Content-Type` declaring a `charset` other than UTF-8 or US-ASCII (a missing,
repeated or malformed one is accepted). `Binary` checks nothing. Typical causes: a mock server or fixture
that sends a bare body, a `204` or `201` without a `Content-Type` on an endpoint with `->`, a
proxy's HTML page, a service that labels JSON `text/plain`.

- *Inspect.* `error.kind() == ErrorKind::Metadata`; `error.codec_error().map(CodecError::label)`
  is a static text such as ``the response Content-Type is not application/json or an
  application/*+json type``; `error.response()` borrows the response (its `Content-Type`), and
  `error.into_response()` takes it with the body unread.
- *React.* Fix the server or the fixture so that it labels what it sends. Where it cannot be
  fixed, select the adapter on **that endpoint** (or a scope of such endpoints), not on the
  client: `response IgnoreContentType<Json>;` skips the `Content-Type` check and nothing else
  (status, other head checks, JSON decoding and the `Accept` suggestion stay). An endpoint that
  answers nothing omits `->`; one that answers JSON or nothing needs a format of its own
  ([Writing a format](CUSTOM_FORMATS.md#7-strict-json-and-the-explicit-adapter)).
- *Check your handlers.* `into_response()` returned only non-2xx responses before; it now also
  returns the 2xx response of a `Metadata` error. Code that treats every `Ok` as an error
  document should look at `error.kind()` or `error.status()` (`Some` for `Status` only) first.
  `ErrorKind` is `#[non_exhaustive]`, so a `match` with a `_` arm compiles and sends a `Metadata`
  error there.

**Typed endpoints send an `Accept`.** An endpoint with `-> T` under `Json` now sends `Accept:
application/json` (under `Form`, `application/x-www-form-urlencoded`) unless the header chain
declares an `accept`, which is sent as written. Before, Concord sent none and reqwest added its
`*/*` (or your client's default `Accept`). Endpoints without `->` and the `Text` and `Binary` formats suggest nothing, as before.
A `headers { ACCEPT = "application/json"; }` that you already had is unchanged.

- *Inspect.* `prepared.request().headers()["accept"]`. A test server that matches the exact header
  set sees a new header.
- *A client-wide default `Accept` is replaced.* reqwest fills in only the header names a request
  lacks. If your `reqwest::Client` was built with `default_headers` containing an `Accept` (an
  API that selects its version that way, `application/vnd.acme.v2+json`), every typed `Json` or
  `Form` endpoint now sends its format's suggestion instead (`application/json`,
  `application/x-www-form-urlencoded`). Declare that `Accept` in the client's
  `headers` block, or use a format that suggests none.
- *Native edits.* After `prepare()?.into_parts()`, the request already carries the suggestion.
  `RequestBuilder::from_parts(..).header(ACCEPT, v)` appends a second `Accept` line; use
  `request.headers_mut().insert(ACCEPT, v)` to replace it.
- *React.* Declare the `Accept` you want; it wins. To drop only the suggestion, use a format that
  suggests none (a small wrapper that forwards the wrapped format's checks and decoding,
  [Writing a format](CUSTOM_FORMATS.md#8-the-accept-suggestion)).
  `remove ACCEPT` is for a header you declared. When Concord sends no `Accept`, reqwest sends its
  own, so no declaration makes the header disappear from the wire. The full table is in the
  [author guide](AUTHOR_GUIDE.md#58-what-goes-on-the-wire-accept-and-content-type).

**JSON error text.** The messages of a failed JSON decode are static labels with positions
(`the body is not valid JSON`, `the JSON does not match the expected type`, `the JSON ends early`;
`line` and `column` through `CodecError::get`). serde_json's own message, which quotes the input,
is only in `source_unredacted()`. A test that matches the old message text matches the label now.
An empty or whitespace-only body is a decode error for every target type (`()` and `Option<T>`
included), with a label that says so; `null` is valid.

**Names.** `concord::BodyError` is gone (`CodecError`), `Error::body_source()` is gone
(`Error::codec_error()`, then `CodecError::source_unredacted()`), and `Decode::decode` is gone
(`decode_body`). `Error::status()` is still `Some` for `Status` only.

**Crates and features.** `concord-core` (the runtime and the open `Encode`/`Decode`/`CodecError`
contracts), `concord-extras` (`Json`, `Text`, `Binary`, `Form`, `IgnoreContentType<F>`, written
against the public traits) and `concord-macros` sit behind `concord`, which re-exports them with
`api!`. Applications keep one dependency. `json` (default) implies `extras`; `Text`, `Binary` and
`IgnoreContentType` need `extras`, `Form` needs `form`.

### A custom `Decode`

Before (`concord-v4-final`, does not compile any more):

```rust,ignore
use concord::bytes::Bytes;
use concord::{BodyError, Decode, Encode, reqwest};

pub struct Lines;

impl Encode<Vec<String>> for Lines {
    const CONTENT_TYPE: Option<&'static str> = Some("text/plain; charset=utf-8");

    fn encode(value: Vec<String>) -> Result<reqwest::Body, BodyError> {
        Ok(reqwest::Body::from(value.join("\n")))
    }
}

impl Decode<Vec<String>> for Lines {
    fn decode(body: Bytes) -> Result<Vec<String>, BodyError> {
        let text = String::from_utf8(body.into())
            .map_err(|e| BodyError::new("the response is not UTF-8").with_source(e))?;
        Ok(text.lines().map(str::to_owned).collect())
    }
}
```

rustc reports errors for the `Decode` impl and points at each (their number and grouping depend
on the compiler version): ``unresolved import
`concord::BodyError` `` (it suggests `CodecError`), ``method `decode` is not a member of trait
`Decode` `` and ``not all trait items implemented, missing: `Context`, `context`, `decode_body` ``
(it prints a skeleton for each).

After:

```rust
use concord::bytes::Bytes;
use concord::{CodecError, ContentTypeField, Decode, Encode, ResponseHead, reqwest};

pub struct Lines;

impl Encode<Vec<String>> for Lines {
    const CONTENT_TYPE: Option<&'static str> = Some("text/plain; charset=utf-8");

    fn encode(value: Vec<String>) -> Result<reqwest::Body, CodecError> {
        Ok(reqwest::Body::from(value.join("\n")))
    }
}

impl Decode<Vec<String>> for Lines {
    type Context = ();

    // New and optional: the format's checks of the response `Content-Type`.
    fn check_content_type(head: &ResponseHead<'_>) -> Result<(), CodecError> {
        match head.content_type() {
            ContentTypeField::One(value) if value.as_bytes().starts_with(b"text/") => Ok(()),
            _ => Err(CodecError::new("the response is not text")),
        }
    }

    // New: every other check of the head, and the state `decode_body` needs. Nothing here.
    fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> {
        Ok(())
    }

    // The old `decode`, with the context in front.
    fn decode_body((): (), body: Bytes) -> Result<Vec<String>, CodecError> {
        let text = String::from_utf8(body.into())
            .map_err(|e| CodecError::new("the response is not UTF-8").with_source(e))?;
        Ok(text.lines().map(str::to_owned).collect())
    }
}

fn main() {
    let body = Bytes::from_static(b"x\ny\n");
    let lines = <Lines as Decode<Vec<String>>>::decode_body((), body).unwrap();
    assert_eq!(lines, ["x", "y"]);
}
```

A translation that keeps the old behaviour is `type Context = ();`, a `context` that returns
`Ok(())`, and the old body renamed `decode_body` with `(): ()` in front; a format that did not
check headers needs no `check_content_type`, and the default accepts every `Content-Type`.

- `CodecError::new` takes a `&'static str`. ``CodecError::new(e.to_string())`` and
  ``CodecError::new(format!(..))`` do not compile (rustc: `expected &str, found String`; its hint
  to borrow only moves the error to `E0716`, a temporary that must live for `'static`). Put the
  dynamic part in a number (`.at("line", n)`, up to three) or in the underlying error
  (`.with_source(e)`), which `Display` and `Debug` never show. A static label is a safer default,
  not a proof: a `String::leak` still compiles.
- Put checks of the `Content-Type` field in `check_content_type` and nothing else there: the
  adapter skips exactly that step. Everything else that needs the head goes in `context`, which
  hands an owned value to `decode_body`. `decode_body` never sees the head; a refusal there is a
  `Decode` error after the body was read, so refuse on head data in `context`.
- `decode_body` called directly performs no head check, by name. Replace
  `<Json as Decode<T>>::decode(bytes)` with `<Json as Decode<T>>::decode_body((), bytes)` where you
  decode an error document yourself.
- `Decoder::new::<F>()` builds a handling for a response you obtained yourself, and
  `ResponseHead::new(status, version, &headers)` lets a unit test build a head
  ([Writing a format](CUSTOM_FORMATS.md#9-testing-a-format)).

### An endpoint whose server mislabels its responses

The deliberate exception, on the one endpoint that needs it:

```rust
use concord::reqwest;
use concord::{IgnoreContentType, Json};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Report {
    pub id: u64,
}

concord::api! {
    pub client Reports {
        base "https://reports.example.com";
        format Json;

        GET latest() -> Report { path "latest"; }
        /// This server sends its JSON as `text/plain`.
        GET legacy() -> Report {
            path "legacy";
            response IgnoreContentType<Json>;
        }
    }
}

fn main() -> Result<(), concord::Error> {
    let api = Reports::new(reqwest::Client::new());
    // Both still suggest `Accept: application/json`; only the response check differs.
    for prepared in [api.latest().prepare()?, api.legacy().prepare()?] {
        assert_eq!(prepared.request().headers()["accept"], "application/json");
    }
    Ok(())
}
```

## From in-place formats to statements

A body or output no longer carries its format in place. Payload positions name Rust types only
(`body b: T`, `-> T`), and the format comes from a `request`/`response`/`format` statement, which
the client, a scope **or the endpoint** can write; the nearest one wins (decision F01,
`v4/finalization/DECISIONS.md`). Generated signatures, `Encode<T>`/`Decode<T>`, status handling
and the native exits are unchanged, so callers need no change.

| In place (does not compile any more) | Statements |
|---|---|
| `json b: T` | `body b: T`, plus `request Json;` on the endpoint unless the nearest statement already says `Json` (without a media type) |
| `json("application/merge-patch+json") b: T` | `body b: T` and `request Json "application/merge-patch+json";` |
| `body("application/problem+json") b: T` | `body b: T` and `request Json "application/problem+json";` — a statement restates its format |
| `Ndjson rows: Vec<Row>`, `text note: String` | `body rows: Vec<Row>` and `request Ndjson;`; `body note: String` and `request Text;` |
| `-> text String`, `-> Csv Rows` | `-> String` and `response Text;`; `-> Rows` and `response Csv;` |
| `request Json("application/vnd.api+json");` | `request Json "application/vnd.api+json";` |
| `request Json; response Json;` | unchanged (or `format Json;`) |
| `GET health() -> text String;` | `GET health() -> String { response Text; }` — a statement needs the endpoint's block |

Migrate by hand: the compiler names the exact replacement at each old form, for example
"removed in-place body format: write `body note: String` and `request Text;` on this endpoint",
"removed output format: write `-> String` and `response Text;` on this endpoint" and
"removed parenthesized media type: write `request Json "application/vnd.api+json";`". A body or
output that no statement reaches is "body `note` has no format: say `request Json;` on
this endpoint, a scope or the client".

Before (`concord-v4-native-repr`, does not compile any more):

```rust,ignore
concord::api! {
    pub client Store {
        base "https://store.example.com";
        request Json;
        response Json;
        PATCH patch(id: u64, json("application/merge-patch+json") patch: Patch) -> Item {
            path "items/{id}";
        }
        GET readme() -> text String { path "readme"; }
        PUT upload(bytes data: Bytes) { path "upload"; }
    }
}
```

After:

```rust
use concord::bytes::Bytes;
use concord::reqwest;
use serde::{Deserialize, Serialize};
use concord::{Binary, Json, Text};

#[derive(Deserialize)]
pub struct Item {
    pub id: u64,
}

#[derive(Serialize)]
pub struct Patch {
    pub name: Option<String>,
}

concord::api! {
    pub client Store {
        base "https://store.example.com";
        format Json;
        PATCH patch(id: u64, body patch: Patch) -> Item {
            path "items/{id}";
            request Json "application/merge-patch+json";
        }
        GET readme() -> String { path "readme"; response Text; }
        PUT upload(body data: Bytes) { path "upload"; request Binary; }
    }
}

fn main() -> Result<(), concord::Error> {
    let store = Store::new(reqwest::Client::new());
    let patch = store.patch(1, Patch { name: None }).prepare()?;
    assert_eq!(patch.request().headers()["content-type"], "application/merge-patch+json");
    let _: concord::Prepared<String> = store.readme().prepare()?;
    let upload = store.upload(Bytes::from_static(b"\x00")).prepare()?;
    assert_eq!(upload.request().headers()["content-type"], "application/octet-stream");
    Ok(())
}
```

**The automatic migration script is withdrawn.** `scripts/migrate_via.py` rewrote source with
regular expressions. Run on the corpus of this campaign, it rewrote a string constant
(`"-> Item via Json"`), a comment and a header literal, and merged two endpoints written on one
line into invalid code. A tool that is safe has to work on tokens, scope its edits to `api!`
invocations, show a diff and refuse ambiguous input; for a change this small the compiler's
replacement messages are the safer route. The script and its check are removed from the repository.

### Native header names

Literal header names keep working. A key can also be a native `HeaderName` constant or static —
standard (`header(AUTHORIZATION)`, `ACCEPT = "..."`), your own
(`const X_RIOT_TOKEN: HeaderName = HeaderName::from_static("x-riot-token")`), imported, aliased
or qualified. Replacing a literal by the equivalent constant changes nothing on the wire:
composition (`override`, `append`, `remove`), the reserved names and the safety list of sensitive
names follow the header a key names, whatever its spelling. The difference is when some mistakes
are reported: in a chain with a native constant, a mistake that depends on which header the
constant holds (setting `"authorization"` below `header(AUTHORIZATION)` without `override`) is a
`Build` error from `prepare()` naming the declaration, not a compile error; mistakes the macro can
prove remain compile errors ([author guide §4.8](AUTHOR_GUIDE.md#48-header-names-and-values)).

### Header values: a closed `ToHeaderValue`

`ToHeaderValue` is now implemented only by Concord: a native `HeaderValue`, `String`/`&str`, the
integers, `bool`, and pointers to them. An `impl ToHeaderValue for MyType` no longer compiles;
send the type's `Display` text with a template (`X_DETAIL = "{detail}"`) or build a `HeaderValue`.
`concord::__private::TextHeader` and the rule "a value placed as a header and as text must be
text" are gone: only a `HeaderValue` can carry its own sensitivity flag, and it is never text.

Header text follows `http` (`HeaderValue::from_str`) instead of Concord's visible-ASCII rule:
UTF-8 beyond ASCII (`café`) is now sent where it used to be a build error (or, as a literal, a
compile error). Control bytes other than horizontal tab are still rejected, as `http` decides —
at compile time for a literal or a template's literal fragment, as a build error for formatted
text.

## From `via` to statements

The body and response syntax of the refined V4 up to commit `c112845` (`body b: T via R`,
`-> T via R`) is replaced by format statements (decision F01,
`v4/finalization/DECISIONS.md`; the intermediate in-place formats are the previous section). Every
body and typed output still has an explicit format, given by the nearest `request`/`response`
statement of the endpoint, a scope or the client. Generated signatures, the `Encode<T>`/`Decode<T>`
traits, status handling and the native exits are unchanged, so callers need no change.

| `via` | Statements | Note |
|---|---|---|
| `-> T via Json` | `-> T` under `response Json;` (or `format Json;`) | the format is a type: `use concord::Json;` (or write `concord::Json`) |
| `-> String via Text`, `-> Bytes via Binary` | `-> String` with `response Text;`, `-> Bytes` with `response Binary;` | on the endpoint, or on a scope that is uniform |
| `body b: T via Json` | `body b: T` under `request Json;` | |
| `body b?: T via my::Csv` | `body b?: T` with `request my::Csv;` | any other format is its type path |
| `use concord::{Binary, Json, Text};` | keep | a statement names the format type, so the import stays (rustc reports an unused one) |
| `headers { "content-type" = "application/merge-patch+json"; }` for one body | `request Json "application/merge-patch+json";` | the media type belongs to the body: sent exactly when the body is |
| a downstream form encoder | `request Form;` (feature `form`) | `serde_urlencoded`, `application/x-www-form-urlencoded` |
| no `->` | no `->` | unchanged: any 2xx, the body is drained, whatever the statements |

Migrate by hand: a leftover `via` is a compile error naming its replacement. Where most endpoints
share a format, state it once on the client (`format Json;`) or on the uniform scopes, and write
the exceptions as statements on their endpoints.

Refined V4 with `via` (does not compile any more):

```rust,ignore
use concord::bytes::Bytes;
use concord::{Binary, Json, Text};

concord::api! {
    pub client Store {
        base "https://store.example.com";
        GET item(id: u64) -> Item via Json { path "items/{id}"; }
        POST create(body item: NewItem via Json) -> Item via Json { path "items"; }
        PATCH patch(id: u64, body patch: Patch via Json) -> Item via Json {
            path "items/{id}";
            headers { "content-type" = "application/merge-patch+json"; }
        }
        GET readme() -> String via Text { path "readme"; }
        PUT upload(body data: Bytes via Binary) { path "upload"; }
        DELETE remove(id: u64) { path "items/{id}"; }
    }
}
```

Statements:

```rust
use concord::bytes::Bytes;
use concord::reqwest;
use serde::{Deserialize, Serialize};
use concord::{Binary, Json, Text};

#[derive(Deserialize)]
pub struct Item {
    pub id: u64,
}

#[derive(Serialize)]
pub struct NewItem {
    pub name: String,
}

#[derive(Serialize)]
pub struct Patch {
    pub name: Option<String>,
}

concord::api! {
    pub client Store {
        base "https://store.example.com";
        format Json;

        GET item(id: u64) -> Item { path "items/{id}"; }
        POST create(body item: NewItem) -> Item { path "items"; }
        PATCH patch(id: u64, body patch: Patch) -> Item {
            path "items/{id}";
            request Json "application/merge-patch+json";
        }
        GET readme() -> String { path "readme"; response Text; }
        PUT upload(body data: Bytes) { path "upload"; request Binary; }
        DELETE remove(id: u64) { path "items/{id}"; }
    }
}

fn main() -> Result<(), concord::Error> {
    let store = Store::new(reqwest::Client::new());
    let patch = store.patch(1, Patch { name: None }).prepare()?;
    assert_eq!(patch.request().headers()["content-type"], "application/merge-patch+json");
    let _: concord::Prepared<String> = store.readme().prepare()?;
    let _: concord::Prepared<()> = store.remove(1).prepare()?;
    Ok(())
}
```

Behaviour changes:

- A format's `Encode::CONTENT_TYPE` that is not a valid header value is a **compile error** at the
  format (it was a `Build` error from `prepare`); a body's media type is computed once at compile
  time instead of being validated on every request.
- A media type declared for a body together with a `content-type` header reaching the same
  endpoint is a compile error ("keep one"). A `content-type` header still replaces a format's own
  media type and is still sent with or without a body.
- The `Content-Type` Concord inserts for a body now carries the `content-type` key's sensitivity
  (`#[sensitive]` or sticky composition); it was never flagged before.
- A custom format with a lowercase single-word name was written as a path (`self::csv`), because a
  bare lowercase word in format position meant `json`, `text`, `bytes` or `form`. That is no longer
  so: a format is an ordinary type path (see
  [From format words to format types](#from-format-words-to-format-types)).
- A leftover `via` is a compile error naming the replacement statement.

## From the V4 alpha (4.0.0-alpha.1 at 8f7adc2) to the refined V4

The version number is unchanged (`4.0.0-alpha.1`); the refinement is the history after commit
`8f7adc2` (tag `concord-v4-alpha1-baseline`). The reasons for each change are recorded in
`v4/refinement/DECISIONS.md` (D01–D18). Breaking changes (the "Refined" column shows the current
statements, which replaced the intermediate `T via R` and in-place spellings):

| Alpha | Refined | Fix |
|---|---|---|
| `Client::new(http, a, b)` | `Client::new(http, ClientConfig { a, b })` | name the required client values |
| `-> Json<T>`, `body b: Json<T>` | `-> T`, `body b: T` under `response Json;`, `request Json;` (or `format Json;`) | the payload type, and a statement for its format |
| `-> Text`, `-> Binary`, `body b: Binary` | `-> String` with `response Text;`, `-> Bytes` with `response Binary;`, `body b: Bytes` with `request Binary;` | same |
| `-> ()` | no `->` | delete it; the output is `()` and the body is drained |
| `impl Encode for R { type Value = T; .. }` | `impl Encode<T> for R { .. }` | traits are generic over the payload; `Decode::DISCARD` is gone |
| `Prepared<Json<T>>`, `Decoder<Json<T>>` | `Prepared<T>`, `Decoder<T>` | name the output type |
| `override "k" = optional;` | `override "k" = optional else inherit;` | required when the source can be absent |
| `"k" = v; remove "k";` in one owner | compile error | `remove` acts first now; delete both statements |
| `error.into_response() -> Option<Response>` | `-> Result<Response, Error>` | `let response = error.into_response()?;` |
| no setter for required values | `set_x` for every value | rename an endpoint that is called `set_<value>` |

Additions that need no change: `#[cfg(..)]` and `#[deprecated(..)]` on scopes and endpoints,
`Decoder::finish`, the format error implements `std::error::Error` (then `BodyError`, now
`CodecError`), endpoint requests are `Clone` exactly when their values and body are.

### Root construction: `{Client}Config`

Required client values were positional, so two values of one type could be swapped silently
(a secret ending up in a host label). They are now the fields of a generated struct, checked by
name.

V4 alpha (does not compile any more):

```rust,ignore
let api = Search::new(reqwest::Client::new(), "app-1", "secret");
```

Refined:

```rust
use concord::reqwest;

concord::api! {
    pub client Search(header("x-app-id") app_id: String, #[sensitive] header("x-api-key") key: String) {
        base "https://search.example.com";
        GET health() { path "health"; }
    }
}

fn main() {
    let config = SearchConfig { app_id: "app-1".into(), key: "secret".into() };
    let api = Search::new(reqwest::Client::new(), config);

    // Every value now has a setter, so rotating a key keeps `with_base_url` and the other values.
    let _rotated = api.set_key("new-secret");
}
```

Config fields take exactly the declared type, so string literals need `.into()`. A missing field
is one `E0063` error naming it. Scope and endpoint arguments stay positional. A client without
required values is still `Client::new(http)`.

### Representations: formats

V4 alpha (does not compile any more):

```rust,ignore
GET item(id: u64) -> Json<Item> { path "items/{id}"; }
POST create(body item: Json<NewItem>) -> Json<Item> { path "items"; }
GET readme() -> Text { path "readme"; }
GET archive() -> Binary { path "archive"; }
PUT upload(body data: Binary) { path "upload"; }
DELETE remove(id: u64) -> () { path "items/{id}"; }
```

Refined:

```rust
use concord::bytes::Bytes;
use concord::reqwest;
use serde::{Deserialize, Serialize};
use concord::{Binary, Json, Text};

#[derive(Deserialize)]
pub struct Item {
    pub id: u64,
}

#[derive(Serialize)]
pub struct NewItem {
    pub name: String,
}

concord::api! {
    pub client Store {
        base "https://store.example.com";
        format Json;
        GET item(id: u64) -> Item { path "items/{id}"; }
        POST create(body item: NewItem) -> Item { path "items"; }
        GET readme() -> String { path "readme"; response Text; }
        GET archive() -> Bytes { path "archive"; response Binary; }
        PUT upload(body data: Bytes) { path "upload"; request Binary; }
        DELETE remove(id: u64) { path "items/{id}"; }
    }
}

fn main() -> Result<(), concord::Error> {
    let store = Store::new(reqwest::Client::new());
    let _: concord::Prepared<Item> = store.item(1).prepare()?;
    let _: concord::Prepared<String> = store.readme().prepare()?;
    let _: concord::Prepared<()> = store.remove(1).prepare()?;
    Ok(())
}
```

`bytes` also accepts `Vec<u8>` in both directions. Generated signatures now name the payload
directly (`send(self) -> Result<Item, Error>`) instead of `<Json<Item> as Decode>::Output`.

The compiler recognizes the alpha spellings (`-> concord::Json<Item>`, `body b: Json<T>`, `-> ()`
without a format) and names the replacement.

### Custom `Encode` / `Decode`

The traits are generic over the payload type and implemented on a marker type; there are no
associated types and no `DISCARD`.

V4 alpha (does not compile any more):

```rust,ignore
pub struct Lines;

impl Encode for Lines {
    type Value = Vec<String>;
    const CONTENT_TYPE: Option<&'static str> = Some("text/plain; charset=utf-8");
    fn encode(value: Vec<String>) -> Result<reqwest::Body, BodyError> { .. }
}

impl Decode for Lines {
    type Output = Vec<String>;
    fn decode(body: Bytes) -> Result<Vec<String>, BodyError> { .. }
}

// declaration: POST normalize(body words: Lines) -> Lines;
```

Refined, written for today's `Decode` (the refined V4 had `fn decode(body: Bytes)` and `BodyError`;
what changed since is in
[From body-only codecs to the codec contract](#from-body-only-codecs-to-the-codec-contract)):

```rust
use concord::bytes::Bytes;
use concord::{CodecError, Decode, Encode, ResponseHead, reqwest};

pub struct Lines;

impl Encode<Vec<String>> for Lines {
    const CONTENT_TYPE: Option<&'static str> = Some("text/plain; charset=utf-8");

    fn encode(value: Vec<String>) -> Result<reqwest::Body, CodecError> {
        Ok(reqwest::Body::from(value.join("\n")))
    }
}

impl Decode<Vec<String>> for Lines {
    type Context = ();
    fn context(_: &ResponseHead<'_>) -> Result<(), CodecError> {
        Ok(())
    }
    fn decode_body((): (), body: Bytes) -> Result<Vec<String>, CodecError> {
        let text = String::from_utf8(body.into()).map_err(|_| CodecError::new("not UTF-8"))?;
        Ok(text.lines().map(str::to_owned).collect())
    }
}

concord::api! {
    pub client Words {
        base "https://words.example.com";
        POST normalize(body words: Vec<String>) -> Vec<String> {
            path "normalize";
            format Lines;
        }
    }
}
```

- A generic alpha representation such as `MyFormat<T>` with a `PhantomData` becomes a plain
  marker with a generic impl: `impl<T: Bound> Decode<T> for MyFormat`.
- A representation that set `DISCARD = true`: declare the endpoint without `->` (the body is
  drained without buffering), or decode and ignore the buffered bytes.

### `Prepared` and `Decoder` name the output type

V4 alpha (does not compile any more):

```rust,ignore
fn log(prepared: &concord::Prepared<Json<Item>>) { .. }
```

Refined:

```rust
pub struct Item;

fn log(prepared: &concord::Prepared<Item>) {
    println!("{prepared:?}"); // redacted rendering
}
```

### Optional overrides say what absence means

The behaviour is unchanged; the declaration must now spell it. An `override` whose source reads
an optional value ends with `else inherit` (absent keeps the inherited value). If you meant "my
value or nothing", write `remove` and set the key again.

V4 alpha (does not compile any more):

```rust,ignore
GET page(page_locale?: String) { query { override "locale" = page_locale; } }
```

Refined:

```rust
use concord::reqwest;

concord::api! {
    pub client Docs(query locale: String = "en") {
        base "https://docs.example.com";

        // Absent: the client's locale is sent (the alpha behaviour).
        GET page(page_locale?: String) {
            path "page";
            query { override "locale" = page_locale else inherit; }
        }

        // Absent: no locale at all.
        GET exact(query locale?: String) {
            path "exact";
            query { remove "locale"; }
        }
    }
}

fn main() -> Result<(), concord::Error> {
    let docs = Docs::new(reqwest::Client::new());
    assert_eq!(docs.page().prepare()?.request().url().query(), Some("locale=en"));
    assert_eq!(docs.exact().prepare()?.request().url().query(), None);
    Ok(())
}
```

`else inherit` on a source that is always present is an error too.

### `remove` acts first within an owner

`remove "k"` now applies to what the owner inherits, before the owner's own placements and
statements. Two consequences:

- `remove` plus a direct placement of the same key in one endpoint is exact replacement (the
  `exact` endpoint above); the alpha rejected it.
- `"k" = v; remove "k";` in the same owner — which in the alpha silently dropped the value it had
  just set — is now an error ("no enclosing owner sets it"). Delete both statements.

### `Error::into_response` returns `Result`

V4 alpha (does not compile any more):

```rust,ignore
if let Some(response) = error.into_response() {
    // decode the error document
}
```

Refined — an error without a response is returned unchanged instead of being dropped (since
4.1, a `Metadata` error has one too: [the codec contract](#from-body-only-codecs-to-the-codec-contract)):

```rust
async fn problem_title(error: concord::Error) -> Result<String, Box<dyn std::error::Error>> {
    let response = error.into_response()?; // an error without a response propagates
    let bytes = response.bytes().await?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
```

### Header values: `ToHeaderValue`, not `Display`

A value placed directly as a header (`header("k") x: T`, `"k" = x`, `"k" = each xs`) is converted
with `concord::ToHeaderValue` instead of being formatted with `Display`. A native
`reqwest::header::HeaderValue` is now accepted and sent byte for byte with its sensitivity flag.
`String`, `&str`, the integers `u8`–`u64`/`usize`/`i8`–`i64`/`isize` and `bool` work as before,
with the same spelling. Path segments, labels, query values and templates are unchanged.

A header value of another `Display` type (a domain enum, `Uuid`, `IpAddr`, a float) is now a
compile error at its type: "`T` cannot be sent as a header value". `ToHeaderValue` is closed
(`HeaderValue`, text, integers, `bool` and pointers to them). Two explicit fixes:

- send its `Display` text with a template (the same spelling as before);
- build a `HeaderValue` yourself and place that.

Before (does not compile any more):

```rust,ignore
GET search(header("x-sort") sort: Sort);
```

After:

```rust
use std::fmt;

use concord::reqwest::{self, header::HeaderValue};

#[derive(Clone, Copy)]
pub enum Sort {
    Newest,
    PriceAsc,
}

impl fmt::Display for Sort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Sort::Newest => "newest",
            Sort::PriceAsc => "price_asc",
        })
    }
}

concord::api! {
    pub client Shop {
        base "https://shop.example.com";
        // A template formats with `Display`, as before:
        GET search(sort: Sort) { path "search"; headers { "x-sort" = "{sort}"; } }
        // Or a native value built by the caller:
        GET browse(header("x-sort") sort: HeaderValue) { path "browse"; }
    }
}

fn main() -> Result<(), concord::Error> {
    let shop = Shop::new(reqwest::Client::new());
    let a = shop.search(Sort::PriceAsc).prepare()?;
    let b = shop.browse(HeaderValue::from_static("price_asc")).prepare()?;
    assert_eq!(a.request().headers()["x-sort"], "price_asc");
    assert_eq!(b.request().headers()["x-sort"], "price_asc");
    Ok(())
}
```

The generated `Debug` of a value placed only as a header shows its header value rather than its
`Display` text (the same text for strings, integers and `bool`).

### Behaviour changes without a syntax change

- A value whose `Display` fails is a build error naming its placement, in every placement (the
  alpha could truncate a segment or panic).
- Literal header names are validated with the native validator at compile time (an over-long name
  panicked in `prepare`), and a request that would exceed the native header-map capacity is a
  build error instead of a panic.
- Only the order of values *within* one header name is part of the contract; the alpha also
  promised an order across names. Query order is unchanged.
- A bodyless endpoint with a move-only value is now accepted (its request is simply not `Clone`);
  an endpoint with a body is `Clone` when the body is.

## From Concord V3 to V4

V4 is a redesign, not a compatible update. It keeps V3's core modeling ideas — one declaration per
API tree, explicit `?` presence, local names with explicit ancestor qualification, compile-time
checked query/header composition, sensitive values — and removes the managed execution runtime.
There is no converter; declarations are short enough to rewrite by hand. V3 is preserved at tag
`concord-v3-preserved`.

### What was removed, and what to do instead

| V3 | V4 |
|---|---|
| `credentials { ... }`, `auth { ... }`, `SecretString`, token refresh | a `#[sensitive]` header or query value: `#[sensitive] header("authorization") token: String`. Refresh by calling `set_token(..)` on the client handle (requests created earlier keep the old value). |
| `rate_limits`, `rate_limit` | outside Concord (e.g. a semaphore around `send()`; see the [recipes](RECIPES.md)). |
| retry policies, `effect` | application code around `send()`; whether a request may be replayed is your decision ([recipes](RECIPES.md)). |
| `paginate`, `.pages()`, `.collect_all()` | call the endpoint again with the next cursor (`maybe_cursor(next)`). |
| `configure_http`, `configure_runtime`, `timeout` | configure the `reqwest::Client` you pass to `new`; per request: `prepare()?.into_parts()` + `timeout_mut()`. |
| built-in base URL only | `base "..."` plus `with_base_url(..)` at runtime. |
| `Client::builder()...build()?` | `Client::new(http, ClientConfig { .. })` — required values are fields of the generated config struct, checked by the compiler. |
| `.await` on a request | `.send().await` (explicit verb). |
| `response Json;` defaults, `via Json` | `format Json;` (or `request Json;` / `response Json;`, with `use concord::Json;`) on the client, a scope or an endpoint, the nearest one winning; payloads name types only (`body b: T`, `-> T`). Without `->` nothing is decoded. |
| `absent on 404` | `match` on `e.status()` ([caller guide](CALLER_GUIDE.md#6-errors)), or `send_raw()`. |
| `status [...]` | any 2xx is success; other statuses are `ErrorKind::Status` with the unread response. |
| `plan()` | `prepare()` gives a native `reqwest::Request`; `Prepared`'s `Debug` is redacted. |
| `replace_x`, `set_x_if_some`, `omit_x`, `reset_x` | `maybe_x(Option<T>)` (exact), `if let`, `maybe_x(None)`, the original handle. |
| `values { ... }` block | client parameters: `client Name(values) { ... }`. |
| `subdomain [region, "api"]` | `subdomain "{region}.api"` (a template). |
| `concat [...]`, `RequestAtom`, `via Adapter` | templates (`"Bearer {token}"`) and `Display` (use a newtype for a custom spelling). |
| `replace "k" = v`, `when_present "k" = v` | `override "k" = v else inherit` (absent keeps the inherited value); exact replacement is `remove "k"; "k" = v;`. |
| `query off`, `path replace/off`, `subdomain replace/off` | not available; restructure scopes. |
| `#[default] x: T` | `x: T = Default::default()`. Initializers may be any expression. |
| Authorization/Cookie/Accept/Content-Type forbidden as public headers | ordinary headers (a declared `content-type` replaces a format's own media type; a body's media type is written with its format, `request Json "application/problem+json";`). |

### Before and after

V3 (does not compile with V4):

```rust,ignore
api! {
    client ShopApi {
        base "https://api.example.com/v1/";
        response Json;
        credentials { secret token; credential bearer_token = bearer(secret.token); }
        auth { bearer bearer_token; }

        GET search(query q?: String, query limit: u32 = 20) -> SearchPage { path "items/search"; }
        POST create(body item: NewItem via Json) -> Item { path "items"; status 201; }
    }
}

let api = ShopApi::builder().token(SecretString::try_new(token)?).build()?;
let page = api.search().set_q("shoe").await?;
```

V4:

```rust
use concord::reqwest;
use serde::{Deserialize, Serialize};
use concord::Json;

#[derive(Deserialize)]
pub struct Item {
    pub id: String,
}

#[derive(Deserialize)]
pub struct SearchPage {
    pub items: Vec<Item>,
    pub next_cursor: Option<String>,
}

#[derive(Serialize)]
pub struct NewItem {
    pub name: String,
}

concord::api! {
    pub client ShopApi(#[sensitive] token: String) {
        base "https://api.example.com/v1";
        headers { "authorization" = "Bearer {token}"; }
        request Json;
        response Json;

        GET search(query q?: String, query limit: u32 = 20) -> SearchPage { path "items/search"; }
        POST create(body item: NewItem) -> Item { path "items"; }
    }
}

async fn search(token: String) -> Result<SearchPage, concord::Error> {
    let api = ShopApi::new(reqwest::Client::new(), ShopApiConfig { token });
    api.search().set_q("shoe").send().await
}
```

### Running V3 and V4 side by side

Depend on V4 under another name; `api!` refers to its own crate through `$crate`:

```toml
concord4 = { package = "concord", path = "...", version = "4.2.0", default-features = false, features = ["extras"] }
```

Excerpt of [`examples/renamed/src/lib.rs`](../examples/renamed/src/lib.rs), which compiles and
tests exactly this:

```rust,ignore
concord4::api! {
    /// A client declared through the renamed crate.
    pub client Renamed(query("v") version: u32 = 4) {
        base "http://renamed.invalid";
        GET ping() -> String { response Text; path "ping"; }
        PUT upload(body data: Vec<u8>) -> Vec<u8> {
            request Binary;
            response concord4::Binary;
            path "upload";
        }
    }
}
```

A format is a type path like any other, so under a renamed dependency it is imported from the
renamed crate (`use concord4::{Binary, Text};`) or written in full (`concord4::Binary`). Compiler
notes still spell the crate `concord` (`concord::Encode<T>`): read it as your dependency's name.
