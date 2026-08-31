# Runtime endpoints

- **Audience:** callers with dynamically known operations and extension authors
- **Purpose:** guide
- **Normative references:** [runtime model](runtime.md), [advanced endpoint types in the public API](generated_client.md)

Runtime-defined endpoints support operations whose exact shape is not available
at macro expansion: configuration-driven paths, service discovery, API
exploration, or plugin-defined operations.

They escape generated syntax, not Concord’s semantic runtime.

## Construct one endpoint

<!-- concord-source: crates/concord-examples/src/runtime_endpoints/generated_and_runtime.rs#runtime_endpoint -->
```rust
pub fn construct() -> Result<(), Box<dyn std::error::Error>> {
    let client = PreparedApi::builder().build()?;
    let endpoint = RuntimeEndpoint::builder(EndpointId::new("prepared_api.lookup")?, Method::POST)
        .path_segment("records")
        .path_segment("42")
        .query("view", "summary")
        .header(HeaderName::from_static("x-example"), "canonical")
        .timeout(Duration::from_secs(5))
        .body_codec::<crate::request_response::specifications::ExampleCodec>(
            "request body".to_owned(),
        )
        .response::<crate::request_response::specifications::ExampleCodec>()
        .build()?;
    let _: Request<_> = client.request(endpoint);
    Ok(())
}
```

The builder selects a bounded endpoint ID, method, public host/path/query/header
shape, body specification, response specification, authentication, rate-policy
plan, and timeout state. Runtime method tokens are limited to 128 bytes and
consume one structural component plus their byte length. A latched error
prevents later formatting or iterator consumption after the first structural
failure.

An endpoint without authentication remains portable between compatible
generated clients. Attaching a generated credential reference binds that
authentication plan to the originating built client and its clones. Planning
or executing it through an independent client build returns
`InvalidAuthentication` before credential-provider or body work.

## Same executor and contracts

Binding a compatible runtime endpoint through `client.request(endpoint)` uses
the same:

- fixed client base;
- public planning;
- hooks and debug;
- credential slots and private placement;
- limiter and retry policy;
- body and response modes;
- deadline and cancellation;
- safe errors and completion.

There is no public second native HTTP path.

## Differences from generated endpoints

Runtime endpoints do not have:

- generated domain navigation;
- compile-time scope inheritance;
- generated value-control methods;
- compiler-derived source names and trait obligations;
- generated pagination bindings.

Dynamic structure is validated at construction/runtime instead of by the DSL
compiler. The API is intentionally more verbose because less information is
available statically.

## Why not expose a native request builder

A native builder would be simpler for unusual protocols, but could bypass
planning, reserved-target validation, bounded body/response contracts, private
credential placement, limiter admission, retry/recovery, hooks, errors,
cancellation, and completion.

The runtime endpoint builder exposes semantic request components while core
retains transport ownership.

## Suitable and unsuitable uses

Suitable:

- discovered links under one known service;
- operator/admin operations supplied by configuration;
- plugin-defined endpoints using existing body/response/auth semantics;
- exploratory tools requiring safe planning.

Potentially unsuitable:

- custom sockets or protocol upgrades;
- exact raw URL/query serialization outside Concord’s model;
- middleware requiring native request/response ownership;
- cryptographic signing over final transport serialization;
- dynamic pagination requiring arbitrary request mutation.

## Related

- [Generated client](generated_client.md)
- [Extensions](extensions.md)
- [Planning and observation](planning_and_observation.md)
