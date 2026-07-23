# Practical guides

The [practical guides](practical-guides.md) cover three complete workflows:

- designing a production REST client from the remote contract;
- modeling a multi-region API without leaking native URLs or dynamic generated rate keys;
- testing generated clients across compile-time, request-shaping, lifecycle, redaction, and feature boundaries.

Runnable source lives in [`concord_examples/examples/full_dsl_reference.rs`](../../concord_examples/examples/full_dsl_reference.rs) and the neighboring example files.

## Runnable example index

| Example | Main subject |
|---|---|
| [`full_dsl_reference.rs`](../../concord_examples/examples/full_dsl_reference.rs) | Complete DSL shape |
| [`authentication.rs`](../../concord_examples/examples/authentication.rs) | Credentials, providers, and provider HTTP |
| [`body_workflows.rs`](../../concord_examples/examples/body_workflows.rs) | Bytes, codecs, streams, and multipart |
| [`generic_specifications.rs`](../../concord_examples/examples/generic_specifications.rs) | Custom request and response specifications |
| [`pagination.rs`](../../concord_examples/examples/pagination.rs) | Built-in and custom pagination |
| [`prepared_endpoint.rs`](../../concord_examples/examples/prepared_endpoint.rs) | Runtime-defined endpoints |
| [`rate_limiting.rs`](../../concord_examples/examples/rate_limiting.rs) | Lexical rules, observers, and explicit keys |
| [`records.rs`](../../concord_examples/examples/records.rs) | CSV, NDJSON, and custom record decoding |
| [`regional_api.rs`](../../concord_examples/examples/regional_api.rs) | Dynamic hosts and regional scopes |
| [`dangerous_raw_response.rs`](../../concord_examples/examples/dangerous_raw_response.rs) | Bounded raw successful responses |
| [`dangerous_custom_retry.rs`](../../concord_examples/examples/dangerous_custom_retry.rs) | Custom native retry configuration |
