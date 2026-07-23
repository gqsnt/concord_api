# Practical guides

> Audience: Users building and testing real clients

These end-to-end guides connect the individual capabilities into complete application patterns.

## Build a production REST client

This guide outlines the decisions required for a production generated client.

### 1. Start with the remote contract

Identify:

- stable base URL and host variation;
- client-wide values;
- scope parameters;
- endpoint parameters;
- body and successful response representations;
- authentication sources and placements;
- pagination state;
- lexical quota ownership;
- finite response sizes and deadlines.

Model structural facts in the DSL. Do not model arbitrary application policy as a macro expression.

### 2. Declare the smallest client

```rust
api! {
    client InventoryApi {
        base "https://inventory.example.com/";
        vars {
            tenant: String;
        }

        path ["v1"];
        query { "tenant" = vars.tenant; }
    }

    GET Item(id: u64) -> Json<Item> {
        path ["items", ep.id];
    }
}
```

Compile before adding authentication or advanced behavior. Macro diagnostics are easier to interpret incrementally.

### 3. Add finite runtime configuration

Choose request, buffered-response, stream, status-body, provider, pagination, and identity limits based on known API envelopes. Do not simply increase all defaults.

```rust
let client = InventoryApi::builder()
    .tenant(tenant)
    .http(|http| http.https_only(true).retry_off())
    .runtime(|runtime| {
        runtime
            .request_body_limit(nonzero(4 * MIB))
            .response_body_limit(nonzero(8 * MIB))
            .stream_response_limit(nonzero(64 * MIB))
            .status_body_limit(nonzero(64 * KIB))
    })
    .build()?;
```

### 4. Add authentication by source

Use static sources only when appropriate. Use OAuth2 or a provider when material expires or must be acquired. Choose challenge policy deliberately; `challenge never` prevents automatic recovery.

### 5. Place rate limits lexically

Put a quota at the nearest subtree it fully covers. Use client, scope, and endpoint rules; do not attempt per-value generated keys. Use a custom limiter or prepared explicit keys for irregular policy.

### 6. Add pagination only to buffered page responses

Model evolving state in controller fields and bind only request-local endpoint/scope fields. Choose a finite `PageLimit` at each call site.

### 7. Handle errors by category

Separate configuration/auth/rate/timeout/transport/status/decode/pagination paths. Access status-body bytes only when the application has a reason to parse them.

### 8. Test the public client

At minimum test:

- generated method and explicit endpoint paths;
- URL/header shaping;
- auth placement at the native boundary;
- request/response limits;
- recovery and cancellation;
- pagination cycle and ceilings;
- limiter plan and observation;
- redaction.

### 9. Document unsupported behavior

State when the client does not support raw responses, prepared pagination, streaming page results, distributed quotas, or other protocol-specific behavior. Users should know the boundary before integrating it.

## Multi-region APIs

Use client values for stable deployment selection and scope parameters for endpoint families whose host or path varies by region.

```rust
api! {
    client RegionalApi {
        base "https://example.test/";
        vars {
            platform: String;
        }
        host [vars.platform, "api"];
    }

    scope Accounts(region: String) {
        host [scope.region, "accounts"];
        path ["accounts"];

        GET ById(id: String) -> Json<Account> {
            path ["by-id", ep.id];
        }
    }
}
```

### Host semantics

A host declaration selects generated labels relative to the fixed base's validated domain structure. Dynamic labels are treated as data and validated before protected work. Authentication is applied only after the final native URL has passed HTTPS policy.

### Region ownership

A scope facade captures the region for endpoint snapshots:

```rust
let eu = client.accounts("europe".to_owned());
let account = eu.by_id("account-42".to_owned()).await?;
```

The client remains immutable; another scope value can be used concurrently.

### Rate limits

A scope rule applies to all descendants regardless of the runtime region value. `accounts("eu")` and `accounts("us")` share the same generated scope declaration bucket when they use the same limiter object. Use a custom limiter for per-region-value authority.

### Security

Do not put credentials into public host/path/query expressions. Place them with `auth`. The safe URL used by hooks excludes native-only query authentication.

### Testing

Use a local proxy or deterministic server to capture the logical request target. Verify each scope shapes the expected host/path and that protected values remain outside automatic diagnostics.

## Testing generated clients

Generated clients should be tested through their public API and deterministic local servers. Avoid tests that only search generated source strings when runtime proof is feasible.

### Layers of proof

#### Compile-time

Use ordinary integration tests and Trybuild for:

- DSL diagnostics;
- feature-disabled types;
- private capability failures;
- generated contract mismatch;
- renamed `concord_core` dependency;
- trait bounds and method availability.

#### Request shaping

Capture logical requests through an ephemeral loopback server or the repository's deterministic test support. Assert method, host, path, query order, public headers, and body boundaries.

#### Lifecycle

Use exact counters and gates for:

- before-send cutoff;
- credential acquisition/finalization;
- limiter waiting;
- recovery;
- response observation;
- completion;
- cancellation.

Do not rely on arbitrary sleeps. Poll futures to the intended pending boundary or use explicit notification/gate objects.

#### Redaction

Use high-entropy sentinels and scan `Display`, `Debug`, recursive sources, hooks, completion, limiter contexts, and Rustdoc. Distinguish intentional recipients such as custom codecs or limiters from automatic surfaces.

### Feature isolation

Workspace feature unification can hide missing-feature behavior. Use standalone fixture packages with `default-features = false`, locked/offline Cargo, and isolated target directories for feature topology.

### Commands

```text
just test
just test-all
just features
just docs
just verify
```

### Avoid empty fixtures

A fixture directory with no manifest or source is not a test. Remove orphaned directories when a test moves into Trybuild or an ordinary integration test.
