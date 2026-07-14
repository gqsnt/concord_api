# Concord API DSL

The macro describes routes, request/response I/O, authentication, rate limits,
profiles, and pagination. General retry is not endpoint syntax: select one
client-level `RetryMode` when constructing the generated client.

## Client

```rust,ignore
api! {
    client UsersApi {
        base "https://api.example.com"
        var tenant: String
        secret api_key: String
        credential key = api_key(secret.api_key)
    }

    GET User(id: u64)
        path ["users", id]
        auth header "X-Api-Key" = key
        -> Json<User>
}
```

`base` is a URL literal. Static host prefixes can remain fixed-origin;
context/endpoint-variable host pieces are descriptor-classified as dynamic.
The generated descriptor records fixed, dynamic, or multi-origin
classification and whether pagination can change origin.

## Routes and parameters

Scopes compose `host` and `path` pieces. Endpoint parameters may feed path,
query, header, timeout, body, and pagination bindings. Optional and defaulted
parameters retain their declared semantics. `fmt(...)` pieces are resolved and
validated semantically before generation.

```rust,ignore
scope tenants(tenant_id: String) {
    path ["tenants", tenant_id]

    GET Search(q?: String)
        path ["users"]
        query { q }
        -> Json<Vec<User>>
}
```

## Request and response I/O

Supported request families include no body, buffered codecs such as `Json<T>`
and `Text<T>`, raw streams, and multipart. Responses may be buffered codecs,
bytes, no-content, or lazy streams. The generated endpoint fixes its response
adapter; callers do not choose a codec at execution time.

Logical body recipes determine only authentication-recovery rebuildability.
They describe authentication-recovery rebuildability; client construction
selects the Reqwest retry mode and determines hidden body cloneability.

## Authentication

Credentials are declared from secrets or endpoint-backed acquisition and are
attached by placement:

```rust,ignore
auth bearer session
auth basic login
auth header "X-Api-Key" = key
auth query "api_key" = key
```

Core owns collision preflight, provider preparation, secret materialization,
generation-safe invalidation, and at most one authentication recovery. A
non-rebuildable body is sent normally; if challenged it follows the original
status path without a second execution.

## Rate-limit policies

Rate-limit declarations may be flat or grouped under `policies`:

```rust,ignore
policies {
    rate_limit tenant {
        bucket method by [host, endpoint, method, "tenant", tenant_key] {
            cost 2
            10 / 1s
        }
    }
    observe rate_limit MyObserver
}
```

Attach a named limit with `rate_limit tenant`, replace inherited limits with
`rate_limit only tenant`, or clear them with `rate_limit off`. A response
observer may translate sanitized response headers into a cooldown for future
calls.

## Profiles and defaults

Profiles bundle authentication and rate-limit attachments. They can extend
other profiles and may be attached at client default, scope, or endpoint
layers.

```rust,ignore
profiles {
    profile tenant_read {
        auth bearer session
        rate_limit tenant
    }
}

default {
    profile tenant_read
}
```

Profile bodies support `auth` and `rate_limit`; no retry clause exists.
Resolved profile names remain documentation metadata, while their semantics
are lowered before code generation.

## Pagination

`paginate` binds a supported or custom controller to endpoint fields. Each
page is a new logical page execution and receives the client's selected
Reqwest retry mode independently. A pagination binding capable of changing
a host component makes client-wide status mode ineligible.

## Generated construction

`GeneratedApi::new(...)` uses `RetryMode::ProtocolRecovery`. Generated clients
also expose constructors for `Disabled` and validated `Status` mode. Generated
source emits descriptor metadata and narrow runtime calls; it emits no retry
loop or delay logic.
