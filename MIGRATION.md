# Migrating to the Concord façade architecture

This guide maps the pre-refactor `0.1.0` workspace surface to the final
contract-compiler and managed-runtime surface. The reviewed repository has no
verified published-release commitment, so the refactor intentionally removes
the temporary dual APIs instead of carrying permanent aliases.

## Use one dependency

Replace direct application dependencies on `concord_core` and
`concord_macros`:

```toml
[dependencies]
concord_core = { version = "0.1.0", features = ["json"] }
concord_macros = "0.1.0"
```

with the user façade:

```toml
[dependencies]
concord = { version = "0.1.0", features = ["json"] }
```

Move ordinary imports to `concord`:

```rust
use concord::{Json, api};
use concord::prelude::{ApiResponse, Bytes, NoContent};
```

Generated code resolves the caller-visible `concord` package, including a
renamed Cargo dependency. A `concord-core` plus `concord-macros` application
setup is no longer accepted.

Feature names are selected on `concord`. The façade forwards transport and
bounded-lifecycle features and composes enabled extras. Its defaults are
`default-tls`, `http2`, and `rate-limit-governor`; JSON remains opt-in.

## Update declarations

Endpoint and scope identifiers now directly define Rust method and module
names and must be `snake_case`:

```rust
scope account_status() {
    GET by_id(id: u64) -> Json<Account> {
        path ["accounts", ep.id];
    }
}
```

Generated type names are deterministic PascalCase transformations:
`account_status` becomes `AccountStatus`; `by_id` becomes `ById` and
`ByIdRequest`.

Named clauses are order-independent. Ordering remains significant inside path,
query, header, authentication-placement, and rate-window lists.

Client values use one `vars` block:

```rust
vars {
    tenant: String;
    locale: Option<String>;
    page_size: u32 = 100;
}
```

Request bodies require the explicit role attribute and may appear at their
declared argument position:

```rust
POST replace(id: u64, #[body] payload: Json<Update>) -> Json<Item>;
```

A parameter named `body` without `#[body]` is an ordinary parameter. Optional
parameters are recognized only as syntactic `Option<T>` or
`std::option::Option<T>`; aliases are not resolved by the macro.

Remove contract-level custom retry and observer selection. Configure
deployment policy through the generated client builder:

```rust
let client = Api::builder()
    .configure_http(|http| configure_http(http))
    .configure_runtime(|runtime| configure_runtime(runtime))
    .build()?;
```

The safe DSL retry choices remain `retry off;` and
`retry reqwest_default;`. Dangerous custom native retry is available only
through the feature-gated HTTP builder.

## Construct configured clients by name

Configured positional constructors are gone. Supply required values, secrets,
and providers through named setters:

```rust
let client = PrivateApi::builder()
    .tenant("west".to_owned())
    .api_token(concord::SecretString::new(token))
    .session_provider(provider)
    .build()?;
```

`Client::new()` exists only for clients with no required values, secrets, or
providers. Missing builder inputs produce a redaction-safe `ClientBuildError`.

OAuth2 client credentials are an ordinary provider rather than macro syntax:

```rust
use concord::auth::OAuth2ClientCredentials;

let oauth = OAuth2ClientCredentials::builder("https://identity.example/token")
    .client_id("client-id")
    .client_secret("client-secret")
    .scope("items.read")
    .build()?;

let client = PrivateApi::builder()
    .oauth_provider(oauth)
    .build()?;
```

Declare its slot generically with
`credential oauth = custom OAuth2ClientCredentials;`. The `api!` compiler does
not recognize OAuth2, JSON, governor, or built-in pagination type names.

OAuth2 builder text setters now borrow through `AsRef<str>` so length is
validated before Concord creates a protected copy. If configuration already
exists as `SecretString`, use the explicit owned setters:

```rust
let oauth = OAuth2ClientCredentials::builder(token_url)
    .client_id_owned(client_id)
    .client_secret_owned(client_secret)
    .scope("items.read")
    .build()?;
```

The public `concord::auth::MAX_OAUTH2_*` constants define the raw and final
Authorization limits. Oversized URL, ID, secret, and scope inputs now produce
new secret-safe `OAuth2ClientCredentialsBuildErrorKind` categories. Encoded
form output can also fail with `AuthErrorKind::RequestTooLarge` when the
client's configured provider request limit is lower.

## Use the scope-oriented generated API

There are no generated `facades` and `endpoints` trees, generated preludes, or
request extension traits. Ordinary capabilities are inherent:

```rust
let request: catalog_api::items::ByIdRequest =
    client.items("books".to_owned()).by_id(42);

let endpoint = catalog_api::items::ById::new("books".to_owned(), 42);
let request: catalog_api::items::ByIdRequest = client.request(endpoint);
```

Both forms bind the same endpoint model and execute through the same managed
lifecycle. Unsupported methods are absent from the named request type.

## Rename response and runtime-endpoint types

Use `ApiResponse<T>` for typed response metadata:

```rust
let response: concord::response::ApiResponse<Item> =
    client.item(42).response().await?;
```

The old `DecodedResponse<T>` alias is removed.

Use `RuntimeEndpoint` and `RuntimeEndpointBuilder` for runtime-defined
endpoints:

```rust
use concord::endpoint::{EndpointId, RuntimeEndpoint};
use concord::http::Method;

let endpoint = RuntimeEndpoint::builder(
    EndpointId::new("inventory.dynamic")?,
    Method::GET,
)
.path_segment("inventory")
.response_bytes()
.build()?;

let bytes = client.request(endpoint).await?;
```

The old prepared-endpoint names are removed. Timeout selection is a real
inherit/explicit/disabled tri-state. Runtime and generated endpoints expose
the same safe description and planning model and use the same executor.

Runtime endpoint query-name parameters now require `AsRef<str>` so the
builder can validate length before copying. `&str`, `String`, and
`Box<str>` continue to work directly. A custom wrapper that implemented only
`Into<String>` must expose an `AsRef<str>` view or pass an owned `String`.
The fluent builder still returns `Self`; its first structural failure is
latched and reported by `build()` without formatting or consuming later
inputs.

## Update advanced rate-limit construction

Runtime rate-limit semantic values are now bounded when they are constructed.
Handle the new fallible key-part and plan constructors:

```rust
use concord::rate_limit::{RateLimitKey, RateLimitKeyPart, RateLimitPlan};

let tenant = RateLimitKeyPart::new("tenant", tenant_id)?;
let key = RateLimitKey::from_parts([tenant])?;
let plan = RateLimitPlan::try_from_rules([rule])?;
```

`RateLimitPlan::from_rules` is removed. Plans reject more than 64 rules and
duplicate rule IDs; rules reject more than 16 windows; IDs, key-part names,
part counts, and aggregate key bytes use the hard ceilings in
`concord::rate_limit`. Iterators stop after the withheld first excess item.

Custom `RateLimiter::preparation_key_limit` values may be lower than the hard
aggregate key cap, including zero for global-only keys, but may not exceed it.
An excessive selected limit now produces a redaction-safe client build error.
`GovernorRateLimiter::key_limit` applies the same hard ceiling.

## Recheck capabilities and safety assumptions

- Planning is sanitized and performs no hooks, credentials, limiter work, body
  encoding/factory work, stream polling, transport, or completion.
- Raw successful responses require `dangerous-raw-response` and remain
  complete and bounded.
- Runtime-defined pagination is intentionally unsupported.
- Authentication recovery remains bounded to one additional attempt.
- Pagination remains sequential, finite, cycle-detecting, and all-or-error.
- Native requests, responses, and authentication material do not cross the
  core boundary.

See the [generated API reference](docs/reference/generated-api.md),
[feature reference](docs/reference/features.md), and
[unsupported forms](docs/reference/unsupported.md) for the final contract.
