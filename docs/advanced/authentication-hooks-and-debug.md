# Advanced authentication, provider HTTP, hooks, and redaction

> Audience: Advanced  
> Covers: custom credential providers, isolated provider HTTP, runtime hooks, debug levels, and extension responsibilities

These APIs integrate application behavior into the managed lifecycle. The application owns its own logging and external effects; Concord retains ordering, native ownership, cancellation, and automatic redaction boundaries.

## Custom credential providers

A `CredentialProvider` supplies typed credential material on demand.

```rust
pub trait CredentialProvider: Send + Sync + 'static {
    type Material: CredentialMaterial;

    fn acquire<'a>(
        &'a self,
        context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>>;

    fn refresh<'a>(
        &'a self,
        current: &'a Self::Material,
        context: CredentialRequest<'a>,
    ) -> BoxFuture<'a, Result<Self::Material, AuthError>>;

    fn recovery_mode(&self) -> RecoveryMode;
}
```

Only `acquire` is required; the default `refresh` delegates to it and the default recovery mode is `Refresh`.

Declare the provider type, not its material type:

```rust
credentials {
    credential session = custom crate::SessionProvider;
}
```

The generated `.session_provider(provider)` builder setter accepts any provider
whose material equals
`<SessionProvider as CredentialProvider>::Material`. The corresponding
credential handle exposes `replace_provider` with the same bound, allowing safe
rotation without giving the macro any provider-specific knowledge.

### Material

`CredentialMaterial` is cloneable, sendable, shared, and may expose an expiration instant. `SecretCredential` additionally exposes the secret string only to private authentication materialization.

Built-in types include `AccessToken`, `ApiKey`, and `BasicCredential`. Custom material should implement redacted `Debug` and `Display` if those traits are exposed.

### Reasons

`CredentialRequest.reason` identifies:

- missing material;
- expired material;
- material expiring within the proactive skew;
- exact-current material rejected by the protected service.

### Single-flight state

Core coordinates one acquisition/refresh leader per credential slot. Waiters share the result, cancellation is handled without stale material restoration, and version checks prevent superseded completions from winning.

### Recovery modes

- `None`: challenge recovery is unavailable.
- `InvalidateOnly`: current material is invalidated but not refreshed automatically.
- `Refresh`: exact-current material may be refreshed once.

### Provider isolation

The context supplies `ProviderHttp`, an isolated finite HTTP capability. Provider traffic does not enter application hooks, application limiting, pagination, application retry, or application request counts.

### Responsibility

Providers own confidentiality of their own storage/logging and must bound retries, maps, queues, tasks, CPU, and external I/O. Provider futures run under the operation deadline and are dropped on cancellation.

## Provider HTTP

`ProviderHttp` is available only inside a credential-provider call. It uses a separate managed client, pool, body limits, and retry policy.

```rust
let request = ProviderRequest::new(Method::POST, token_url)
    .header(CONTENT_TYPE, HeaderValue::from_static("application/x-www-form-urlencoded"))
    .body(ProviderBody::Bytes(body))
    .timeout(Duration::from_secs(2));

let response = context.http.send(request).await?;
```

### Request API

`ProviderRequest` supports:

- method and URL;
- headers;
- empty, complete bytes, or synchronous factory body;
- request-local timeout;
- a response limit that can only reduce the client-wide finite provider response limit.

### Response API

`ProviderResponse` contains actual status, headers, and complete bounded bytes. Non-success statuses are returned to the provider rather than automatically becoming an application `HttpStatus` error; the provider owns token-service semantics.

### Limits and deadlines

Provider request and response limits default to 1 MiB each. The credential operation deadline applies, and a shorter request-local timeout may be selected. Provider body factory output is checked after synchronous return.

### Isolation

Provider traffic is excluded from:

- application before-send/response/completion hooks;
- application limiter acquisition and observation;
- application pagination;
- application retry configuration;
- application request debug and operation identity.

Client TLS roots are shared as configured, but an application client identity PEM is not installed on the provider client.

### What cannot be done

The provider cannot obtain the application native client, insert work into the application lifecycle, increase finite limits, or bypass cancellation and deadline ownership.

## Runtime hooks

`RuntimeHooks` observes the managed lifecycle through sanitized, body-free events.

```rust
pub trait RuntimeHooks: Send + Sync + 'static {
    fn before_send<'a>(&'a self, event: BeforeSend<'a>)
        -> BoxFuture<'a, Result<(), HookError>>;
    fn on_response_head<'a>(&'a self, event: ResponseHeadEvent<'a>)
        -> BoxFuture<'a, ()>;
    fn on_complete<'a>(&'a self, event: CompletionEvent<'a>)
        -> BoxFuture<'a, ()>;
    fn on_pagination_complete<'a>(&'a self, event: PaginationCompletionEvent<'a>)
        -> BoxFuture<'a, ()>;
}
```

Install:

```rust
client_builder.configure_runtime(|runtime| runtime.hooks(Arc::new(MyHooks)))
```

### `before_send`

Runs after public preparation and body metadata but before body preparation, credentials, limiting, authentication, or native send. It can veto the attempt with `HookError`.

The event includes operation IDs, endpoint, method, safe URL, page/recovery index, sanitized public headers, structural body kind, and known body length.

### `on_response_head`

Runs for the final response head of an attempt before challenge/status/body handling. It cannot replace or suppress the response.

### Completion

Direct/page completion observes an already fixed result outside the operation deadline. Aggregate pagination completion is separate. Completion events contain structural outcome, status, counts, elapsed time, and recovery state—not bodies or credentials.

### Concurrency and cancellation

Calls may overlap. Result-determining hook futures are dropped on cancellation. Completion hooks are not automatically time-limited; they must bound their own work. Already completed logging, metric, or external side effects are not rolled back.

### What hooks cannot do

Hooks cannot mutate the native request, provide credentials, change the limiter, decode the response, change status handling, replace completion, or access request/response bodies.

## Debug and redaction

Concord's built-in debug and error surfaces are structural. They are designed to help identify the operation without reproducing protected values.

### Debug levels

- `DebugLevel::None`
- `DebugLevel::Basic`
- `DebugLevel::Headers`

Set a client default through `RuntimeConfigBuilder::debug` and override one request with `.debug(level)`.

`Headers` includes sanitized public request and response headers. Authentication headers and native query authentication are not public headers and are never included.

### Values excluded from automatic surfaces

- secrets and credential material;
- authentication header/query values;
- request bodies and multipart values;
- buffered/stream/status/raw response bodies;
- stream chunks and record values;
- pagination continuation and collected items;
- explicit advanced rate-key values;
- native authenticated URLs;
- provider request/response values.

### Structural wrappers

`ApiResponse<T>::Debug` does not format `T`. `ByteStream`, `RecordStream`, `RecordBatches`, `RequestBody`, multipart recipes, credentials, and raw response debug output avoid body/secret values. `RawResponse::Debug` reports body length, not bytes.

### Intentional recipients

Some extensions intentionally receive protected values:

- codecs receive body bytes;
- credential providers receive credential inputs/material;
- selected custom limiters receive explicit key parts;
- record decoders receive record bytes;
- raw-response callers receive bounded successful bytes.

Those extensions own logging and storage safety. Concord cannot redact application-created logs.

### Safe error design

Use error kinds and safe constant messages. Avoid embedding parser errors, raw HTTP values, filenames, tokens, keys, records, or payload fragments. Recursive sources are also scanned by tests and should remain value-free.

### Threat model

Redaction reduces accidental disclosure through built-in surfaces. It is not encryption, access control, memory zeroization, or protection from an application that explicitly calls secret/body accessors.
