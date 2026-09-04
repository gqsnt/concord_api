# Runtime-defined endpoints

Runtime endpoints are a pressure valve for operations discovered or configured at runtime under an existing Concord client/scope context. They are not a native HTTP escape.

## Suitable uses

- discovered relative resources under a fixed service origin;
- plugin/configuration-defined operations using Concord representations;
- rare/admin/test operations not worth static generation;
- dynamic local query/header/value structure that still fits the managed executor.

Unsuitable:

- arbitrary external absolute URLs under an existing client’s credentials;
- CONNECT, TRACE, WebSocket, upgrade, cookies, or caching;
- raw signing over final native serialization;
- unrestricted middleware/native request/response mutation;
- bypassing auth/rate/deadline/body/response limits.

## Builder

<!-- concord-source: crates/concord-examples/src/runtime_endpoints/generated_and_runtime.rs#runtime_endpoint -->
```rust
pub fn construct() -> Result<(), Box<dyn std::error::Error>> {
    let client = PreparedApi::builder().build()?;
    let endpoint = RuntimeEndpoint::builder(EndpointId::new("prepared_api.lookup")?, Method::POST)
        .path("records")
        .path("42")
        .query("view", "summary")
        .header(HeaderName::from_static("x-example"), "canonical")
        .timeout(Duration::from_secs(5))
        .body_codec::<crate::request_response::specifications::ExampleCodec>(
            "request body".to_owned(),
        )
        .response::<crate::request_response::specifications::ExampleCodec, String>()
        .build()?;
    let _: Request<_> = endpoint.try_bind(&client)?;
    Ok(())
}
```

## Binding

A runtime endpoint binds fallibly through a built client root or generated selected scope:

```text
let request = endpoint.try_bind(&client)?;
let request = endpoint.try_bind(&regional_scope)?;
```

The binding owner supplies:

- fixed/effective logical origin and base path;
- inherited public patches;
- inherited representation defaults/fences where typed compatibility permits;
- auth, timeout, and additive rate policies;
- runtime/security context.

The runtime endpoint contributes validated local operations. It cannot select a scope by string, replace the logical origin, or add arbitrary dynamic subdomains.

Generated unbound binding remains separate and infallible:

```text
client.request(generated_intent)
```

Do not weaken it to match runtime endpoints.

## Patch parity

Runtime endpoints must be able to express the same local meanings:

- scalar/optional/ordered assignment;
- conditional override;
- append;
- remove;
- family replace/off;
- explicit atom adapters;
- ordinary versus sensitive public values;
- body/response representation;
- status, effect, finite timeout;
- private auth/rate capability through validated references.

They do not receive compile-time domain navigation or generated pagination. Automatic runtime pagination remains deferred.

## Sensitivity

Plain runtime values are ordinary public. Use a bounded wrapper:

```text
RuntimeValue::visible(value)
RuntimeValue::sensitive(value)
```

The wrapper cannot reveal inner data through `Debug`. Real and sanitized atoms derive from one encoding pass. It is not for credentials or bodies.

## Auth/security context

A runtime endpoint carrying a credential reference is bound to the originating client security context. Binding under another independently built client fails before body/provider/limiter work.

## Same executor

After binding, runtime endpoints use the same:

- public atom/patch machinery;
- reserved-target and origin checks;
- representation/body/response lifecycle;
- auth/rate/retry/effect/deadline gates;
- errors, plans, observer, and completion;
- native HTTP executor.

No `.raw_send`, native builder accessor, or second transport path exists.

## Latched builder errors

A first-error-latched builder is acceptable for fallible iterator insertion. Once latched, it must stop consuming potentially effectful/custom iterators. `build()` returns the original structured error with operation context.

## Evidence

- fixed-origin binding at client and scope;
- complete query/header patch parity;
- sensitivity for scalar/optional/repeated values;
- security-context mismatch before private/body/rate work;
- same runtime trace and limits as generated endpoints;
- no raw/native escape;
- first-error latch stops consumption;
- no automatic pagination.
