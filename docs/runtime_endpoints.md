# Runtime-defined endpoints

Runtime endpoints are the dynamic-contract escape route. They are not a native
HTTP escape route and cannot mutate generated client/scope values.

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
        .response::<crate::request_response::specifications::ExampleCodec>()
        .build()?;
    let _: Request<_> = client.request(endpoint);
    Ok(())
}
```

Construction is inert and first-error latched. Once one operation fails, later
values/iterators are not formatted or consumed. Component and public-byte limits
apply while building, and route/head limits are shared with generated requests.

The builder uses v2 vocabulary: validated `subdomain`, `replace_subdomains`, and
`subdomain_off`; validated/once-encoded `path`, `replace_path`, and `path_off`;
ordered query assignment/append/removal/off; header replacement/removal/off;
auth add/replace/off; timeout selection; explicit `Repeatability`;
response-owned `accept`/off; body/response specs; canonical rate policies; and
explicit accepted-byte response selection.

`client.plan(&endpoint)` uses the same public shaping and returns the same
credential-free plan model. `client.request(endpoint)` binds it to that client's
transport, security identity, credential context, limiter, retry policy, gates,
observer, limits, and executor. Credential references from another independent
build are rejected before private/native work.

Runtime endpoints have a value-owned `EndpointDescription`. Their runtime-bound
shape is represented honestly rather than pretending arbitrary extension code
is statically known. They obey the same deadline, retry/replay, auth recovery,
rate admission, response acquisition, redaction, cancellation, and error laws.

Runtime pagination is intentionally absent unless it can preserve the same
generated slot-authority and aggregate-bound model; callers should prefer a
generated paginated contract rather than arbitrary mutation.
