# Unsupported forms

Concord deliberately omits capabilities that would bypass its typed contract,
private transport boundary, finite acquisition rules, or coherent operation
accounting. This page describes the current checkout; absence is part of the
public contract.

## Transport and acquisition

| Unsupported form | Supported direction |
|---|---|
| native request, response, client, middleware, or transport escape | use public shaping, body/response specifications, hooks, and typed errors |
| exact authenticated/native request preview | use `SafeRequestPlan` for the real public head and test private placement at controlled loopback |
| unbounded request, buffered response, stream, status body, record staging, or provider HTTP | configure evidence-based finite limits within hard structural ceilings |
| automatic redirects or redirect interception | handle the typed terminal 3xx status according to application policy |
| a second executor for runtime endpoints, providers, pagination, explicit response bytes, or extensions | use the one managed lifecycle and its semantic extension contracts |
| arbitrary native response modes | select the sealed `Buffered`, `Streaming`, or `Discard` mode |
| WebSocket management | use a protocol-specific client outside Concord |
| an SSE reconnection framework | consume a bounded byte/record stream and own a finite application reconnection policy |
| multipart response parsing | implement an appropriate bounded response/record design outside the request-only multipart helper |

`response_bytes()` stays inside this boundary. It returns accepted response
headers and complete bounded bytes in a Concord-owned `ApiResponse<Bytes>`,
not a Reqwest response or live native body.

## Authentication

Concord does not support:

- unlimited authentication recovery;
- arbitrary access to the final request from a credential provider;
- complete custom `Authorization` schemes such as `Signature`,
  `AWS4-HMAC-SHA256`, or `MAC`;
- placing `Authorization` through a generic public or private custom-header
  clause;
- secret or credential expressions in public URL/header shaping;
- automatic propagation of the application client identity to provider HTTP.

Bearer and Basic are the dedicated `Authorization` owners. API keys and other
single-string material can use a private custom header or query placement.
Supporting another complete authorization or request-signing scheme needs a
dedicated semantic design that preserves one private owner, collision
prevention, limiter-before-placement ordering, replay semantics, and
redaction.

Do not work around the limitation with a public header. That would expose the
value to plans, hooks, descriptions, pagination identity, and other public
surfaces.

Provider HTTP is the supported network capability for credential acquisition.
It is isolated, HTTPS-only, bounded, and does not expose native signing state.

## Pagination and multi-request protocols

Generated pagination is sequential, typed, cycle-detecting, bounded, and
all-or-error. Concord does not provide:

- parallel page execution;
- implicit partial results after a later page failure;
- a resumable page-stream terminal;
- pagination over streaming, record, discard, or explicit response-bytes
  acquisition;
- pagination with a one-shot stream, `RequestBody`, multipart, or another
  request specification that lacks `PageRequestBodySpec`;
- inferring repeatability from `Clone`, `RequestBodySpec`, or `EncodeBody`
  without the trusted page-stability marker;
- pagination-controller bindings that mutate a `#[body]` parameter;
- runtime-defined pagination without a coherent generated binding contract;
- controller access to transport or external I/O.

Use `PageLimit::Take` for an explicitly accepted prefix. Use an
application-owned finite loop when the protocol requires partial delivery,
parallelism, checkpoints, or dynamic multi-request transitions. That loop is
not a hidden Concord pagination terminal and must own its own identity,
limits, completion, and error semantics.

## Rate limiting

The DSL does not infer quota identity. It supports only `host`, `endpoint`, and
required/defaulted scalar `vars.*`, visible `scope.*`, or `ep.*` dimensions.
It does not support:

- `method`, path, query, full/safe URL, body, literal, secret, or credential
  dimensions;
- optional, vector, optional-vector, formatted, or
  pagination-controlled value dimensions;
- `host` partitioning when final host shaping depends on a
  pagination-controlled value;
- descendant replacement or disabling of an inherited rule;
- explicit user-authored generated rate-rule IDs;
- repeatable named rules at one lexical owner;
- a built-in observer chain;
- implicit process-global limiter state;
- a first-party distributed strict-quota implementation.

Omitting `by` is the declaration-global form; there is no `by global`
spelling. Rules from client, scopes, and endpoint are additive.

Install a custom bounded `RateLimiter` for distributed or non-token-bucket
policy. Compose multiple response policies inside one custom
`RateLimitObserver` if an observer chain is needed. Sharing one limiter across
unrelated generated contracts requires deliberately compatible rule
namespaces.

## Compiler and generated surface

The declaration language does not provide:

- macro recognition of concrete codec, response, provider, limiter, observer,
  or pagination-controller names;
- arbitrary Rust expressions, calls, indexing, methods, macros, closures, or
  blocks in public shaping;
- proc-macro type-alias resolution for optionality;
- endpoint aliases separate from the source method name;
- custom visibility clauses;
- positional secret/provider client construction;
- legacy bare value setters, `with_*` aliases, or mutable request setters;
- trait-import-dependent ordinary generated methods;
- application implementations of the sealed generated `Endpoint` trait;
- public access to the hidden generated-code bridge.

Use ordinary Rust types behind public semantic traits. Put an `api!` invocation
in the Rust module that should own its visibility. Choose the final endpoint
source name directly, and use `set_*`, `clear_*`, and `reset_*` according to
the generated capability matrix.

## Runtime-defined endpoints

`RuntimeEndpoint` supports dynamic public method, host, path, query, headers,
body specification, response specification, and timeout selection. It still
uses:

- a validated `EndpointId`;
- restricted bounded public values;
- the same client base and runtime;
- typed body and response specifications;
- ordinary hooks, authentication boundary, limiter, status, acquisition, and
  completion.

It does not supply native mutation, custom auth signing, unbounded values, a
different executor, or partial runtime pagination.

## Choosing an extension

Supported semantic extension families are:

- `EncodeBody` / `DecodeBody`;
- `RequestBodySpec` / `ResponseBodySpec`;
- `RecordDecoder`;
- `CredentialProvider`;
- `PaginationController`;
- `RateLimiter` / `RateLimitObserver`;
- `RuntimeHooks`.

If the required implementation cannot fit one of those contracts without
native transport ownership, unlimited state, or hidden lifecycle work, it is
outside Concord's current managed-runtime model.

## Related

- [Extension model](../extending/README.md)
- [Complete DSL reference](dsl.md)
- [Generated API reference](generated-api.md)
- [Finite behavior](../concepts/finite-behavior.md)
