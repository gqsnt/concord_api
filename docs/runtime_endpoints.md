# Runtime-defined endpoints

A `RuntimeEndpoint<R, O>` is a typed endpoint you build manually at runtime and bind under an existing generated client root or scope. It is a pressure valve for operations that are discovered or configured at runtime. It is **not** an escape from the declaration: a runtime endpoint inherits the owner's fixed origin, public shaping, private authentication, timeout and every rate constraint, exactly as a generated endpoint of that owner does (spec §16, RUNTIME-01).

## Suitable uses

- discovered relative resources under a fixed service origin;
- plugin- or configuration-defined operations using Concord representations;
- rare admin or test operations not worth declaring;
- dynamic local query/header structure that still fits the managed executor.

Unsuitable:

- external absolute URLs, other origins, schemes or ports under a client's credentials;
- `CONNECT`, `TRACE`, WebSocket, upgrade, cookies or caching;
- raw signing over final native serialization;
- native request/response mutation or a second transport path;
- choosing another credential source, or bypassing authentication, rate, deadline, body or response limits.

## The owner

Runtime endpoints bind at generated owners. Any client root or scope handle can be the owner:

<!-- concord-source: crates/concord-examples/src/runtime_endpoints/generated_and_runtime.rs#runtime_endpoint_client -->
```rust
api! {
    client PreparedApi {
        base "http://prepared.example.test/";
        timeout 30s;

        GET declared() -> () via Empty;

        scope regional(region: String) {
            timeout 10s;
            path "regions/{region}";
            headers { "X-Region" = region; }

            GET declared() -> () via Empty;
        }
    }
}
```

## Construction and binding

<!-- concord-source: crates/concord-examples/src/runtime_endpoints/generated_and_runtime.rs#runtime_endpoint -->
```rust
/// Builds one runtime-defined endpoint descriptor.
///
/// Construction records a symbolic program: no value is encoded, no body is
/// prepared and nothing is sent until the endpoint is bound and executed.
///
/// # Errors
///
/// The first latched structural error (invalid ID, key, method or a
/// repeated singleton selection).
pub fn lookup() -> Result<RuntimeEndpoint<Text, String>, ConfigurationError> {
    RuntimeEndpoint::builder(EndpointId::new("prepared_api.lookup")?, Method::POST)
        .path("records")
        .path("42")
        .query("view", "summary")
        .header("x-example", "canonical")
        .timeout(Duration::from_secs(5))
        .body_repr::<Text, _>("request body".to_owned())
        .response::<Text, String>()
        .build()
}

/// Binds the runtime endpoint at the client root and at a generated scope,
/// and returns the scope's inert plan URL.
///
/// The scope binding inherits the scope's path, headers and timeout exactly
/// as its generated `declared()` endpoint does; the runtime-local timeout
/// selection still wins.
///
/// # Errors
///
/// Client construction, descriptor construction, or binding failures.
pub fn construct() -> Result<String, Box<dyn std::error::Error>> {
    let client = PreparedApi::builder().build()?;
    let _root: BoundRuntimeRequest<Text, String> = client.bind(lookup()?)?;

    let scope = client.regional("eu");
    let plan: RequestPlan = scope.plan_runtime(&lookup()?)?;
    let _scoped: BoundRuntimeRequest<Text, String> = scope.bind(lookup()?)?;
    Ok(plan.observed_url().to_owned())
}
```

`RuntimeEndpoint::builder(id, method)` is an associated function on the default factory `RuntimeEndpoint<(), ()>`. It returns a builder with **no response selection**. Exactly one `.response::<R, O>()` call selects the typed output pair, and only a builder with a valid `R: ResponseRepresentation<O>` can `.build()`. No representation is guessed from the output type.

Every generated owner has two methods:

```text
owner.bind(endpoint)        -> Result<BoundRuntimeRequest<R, O>, ApiError>
owner.plan_runtime(&endpoint) -> Result<RequestPlan, ApiError>
```

Binding at the root uses the same projection as binding at a scope. There is no root-only bare-URL path. The owner supplies:

- the fixed origin and base path, generated host labels and the owner's generated path segments;
- the owner's complete symbolic query/header program;
- every effective private authentication placement;
- the owner's effective timeout, unless the runtime endpoint selects its own;
- every inherited rate policy, plus the client's limiter and runtime context.

`BoundRuntimeRequest<R, O>` has the same ordinary, response and control interfaces as a generated request. It implements `IntoFuture<Output = Result<O, ApiError>>` and `RequestControls`. It provides `response()`, `plan()`, `description()`, `timeout(d)`, `reset_timeout()`, `cancel_with(token)`, `with_options(&options)` and `control_summary()`. `accepted_bytes()` is available when the descriptor called `expose_accepted_bytes()`. The request is inert until first polled and runs in the same managed executor. It is not `Clone`, because it owns its body input; to bind one definition repeatedly, keep a factory closure.

Generated unbound intents keep their separate, infallible, type-checked binding (`client.request(intent)`). Runtime binding is fallible because it validates at runtime what the macro validates at compile time.

## Operation families (§16.2)

| Family | Methods |
|---|---|
| Path | `path(v)`, `path_via::<T, A>(v)`, `path_sensitive(v)`, `path_sensitive_via::<T, A>(v)`, `path_replace()`, `path_off()` |
| Query | `query`, `query_optional`, `query_each`, `query_each_optional`, `query_replace`, `query_replace_optional`, `query_replace_each`, `query_replace_each_optional`, `query_when_present`, `query_when_present_each`, `query_append`, `query_append_optional`, `query_append_each`, `query_append_each_optional`, `query_remove`, `query_replace_all`, `query_off` |
| Headers | the same names with `header` (`header` is the plain scalar), `header_remove`, `header_replace_all`, `header_off` |
| Modifiers | every keyed value method also has `_via::<T, A>` (explicit atom adapter), `_sensitive` and `_sensitive_via::<T, A>` forms |
| Body | `body_repr::<B, T>(input)`, or no body |
| Response | `response::<R, O>()`; `expose_accepted_bytes()` for buffered/discard drivers |
| Status/effect/time | `status(AcceptedStatus)`, `status_codes(iter)`, `effect(Effect)`, `timeout(Duration)` |
| Rate | `rate_limit(RateLimitDeclaration)` (additive only) |
| Auth | `auth_off()`, `auth_remove(AuthTarget)` |

Value arguments are owned. Scalars are `T: RequestAtom + Send + Sync + 'static` (string literals qualify). Optionals are exact `Option<T>`, where outer presence is read from the slot and never from an adapter. `each` sources are `V: OrderedValues + Send + Sync + 'static`. Keys are `&str`, copied at construction. Query keys are nonempty, case-sensitive and at most `MAX_RUNTIME_KEY_BYTES` (256) bytes. Header names are valid field-name tokens of at most 256 bytes, compared case-insensitively.

There is no runtime `absent on`; use the explicit buffered representation pattern (§10.3). There is no runtime pagination; typed generated endpoints provide the page surface.

## One evaluator, identical semantics

A runtime endpoint records a **symbolic program**. Construction and binding never encode a value, invoke an adapter, iterate a sequence, prepare a body, acquire a credential, reserve quota or touch the transport. At plan or execution time, the runtime program is evaluated as one more program segment after the owner chain's segments, by the same evaluator generated endpoints use. Suppression, ordering and sticky sensitivity are therefore identical to static code (PATCH-01..03):

```text
owner program: a=1, locale=fr, z=9
query_when_present("locale", None)            => a=1, locale=fr, z=9
query_when_present("locale", Some("de"))      => a=1, z=9, locale=de
query_replace_optional("locale", None)        => a=1, z=9
query_append_optional("locale", Some("de"))   => a=1, locale=fr, z=9, locale=de
query_when_present_each("locale", Some([]))   => a=1, z=9
query_replace_all().query("b", "2")           => b=2
query_off()                                   => (no query)
```

A suppressed owner emission is never encoded, so an owner adapter that would fail cannot fail the request. Each surviving scalar is encoded once per preparation. Each item of a surviving `each` source is encoded once, in order. An absent source is not iterated. Plan and execution are separate preparations and may encode again.

## Structural rules

The static structural rules apply unchanged. Local violations are rejected at `build()`, and violations that depend on the owner are rejected at `bind`/`plan_runtime`, before any value is encoded:

- a plain introduction (`query`, `query_optional`, `query_each`, …) may not target a key with an earlier potential writer in the owner chain or in this program. This holds for every source shape, including an absent optional (PATCH-02);
- `*_replace*`, `*_when_present*` and `*_remove` require an earlier potential writer, which catches misspelled override targets;
- `query_replace_all()`/`query_off()` (and the header forms) must come before every same-family statement and appear at most once. `query_off()` accepts no later statement in its family. `path_replace()`/`path_off()` follow the same rule for path atoms;
- singleton request headers (`User-Agent`, `Referer`, `Range`, …) cannot repeat through `*_append*` or `*_each*`;
- transport-, media- and authentication-owned header names (`Host`, `Authorization`, `Content-Type`, `Cookie`, …) are never runtime targets. Neither is a name or key claimed by an effective private placement, even when the runtime source is absent (HEAD-01);
- endpoint-local selections (`timeout`, `status`/`status_codes`, `effect`, `body_repr`, `expose_accepted_bytes`, `auth_off`) are singletons, not last-writer-wins;
- the method is a token of at most 128 bytes; `CONNECT` and `TRACE` are rejected.

A path atom is exactly one segment. It must be nonempty, not `.` or `..`, and contain no `/`, backslash or pre-encoded escape. It is percent-encoded once. There is no operation that supplies an origin, scheme, port or absolute URL.

## First-error latch and limits

The builder records at most `MAX_RUNTIME_ENDPOINT_COMPONENTS` operations (the method counts as one) and `MAX_RUNTIME_ENDPOINT_PUBLIC_BYTES` library-retained descriptor bytes (method, keys, rate declarations, authentication targets). The first structural or limit error is **latched**. Every later call drops its argument immediately, without retaining, iterating or encoding it, and `build()` returns that first error. Errors carry a category only, never values or rate-key bytes. At plan or execution, `each` iteration stops at the first head-limit excess, and nothing past it is read.

## Sensitivity

Every keyed method and `path` has a `_sensitive` form. The real request carries the value; automatic observation (plans, errors, observers, debug) redacts it. A sensitive path segment is replaced as a whole. Sensitivity is sticky along the owner-to-runtime chain: a runtime public override cannot declassify a key the owner classified sensitive. Mandatory name-based redaction (`token`, `secret`, `api-key`, …) applies in addition. Sensitive classification is not for credentials; credentials come only from the owner's declared placements.

## Authentication

A runtime endpoint inherits every effective placement of its owner. It can only remove authentication:

```text
.auth_off()                                    // remove every inherited placement
.auth_remove(AuthTarget::authorization())      // Bearer/Basic/Authorization placement
.auth_remove(AuthTarget::header("X-Api-Key")?) // private header placement
.auth_remove(AuthTarget::query("api_key")?)    // private query placement
```

A removal names the placement by its safe target identity. Binding fails if no effective placement owns that target. No runtime API can create a private target, select another credential source, change a challenge, or reintroduce a removed placement. Declare a scope with the required `auth replace { … }` and bind there to select a different credential source.

Removing inherited authentication changes the request's declared security behavior. It is displayed prominently: `RequestPlan`'s `Debug` output starts with `runtime_authentication: INHERITED AUTHENTICATION REMOVED …` (or `… PARTIALLY REMOVED …`, naming the removed targets). `description().runtime_binding().authentication()` reports the disposition, and the plan's capabilities report the effective authentication. It is never described as preserving the owner's authorization semantics (RUNTIME-02).

Binding also validates that an explicit accepted status set does not overlap an effective challenge (for example `status_codes([200, 401])` under a `challenge unauthorized` placement). Credentials are sent only over HTTPS: binding a credential-carrying request at a cleartext base fails before any observer, body or transport work.

## Rate policies

`rate_limit(declaration)` adds an explicitly identified, explicitly labelled policy. Built-in `origin()`/`endpoint()` dimensions resolve from the bound head. Runtime policies are additive: every inherited owner policy remains, and no operation removes one. The duplicate rules of generated attachments apply (§12.2). An exact repetition (same ID, cost, windows and ordered dimensions) is deduplicated, and any other repetition of an ID is rejected, both among runtime policies and against the owner's policies at bind. A runtime policy on a client built without a limiter selection fails at bind; it never reaches the fail-closed fallback or bypasses enforcement. An explicit `NoopRateLimiter` is disclosed in the plan's enforcement label.

## Identity

The runtime `EndpointId` is checked at bind against the client's generated endpoint IDs, and a collision is rejected. Runtime IDs are not stored in a global registry; callers own the consistent use of their runtime IDs.

## Descriptions

`endpoint.description()` and `bound.description()` are value-free. `runtime_binding()` returns `RuntimeBindingFacts`, which contain:

- the binding owner (for example `PreparedApi::regional`);
- the route and family modes;
- the ordered keyed operations, with key names, sensitivity, shape and adapter type;
- the authentication disposition and effective placement count;
- the runtime rate policy IDs and the effective policy count.

The bound description's source path is `<Owner>::<runtime>`, and its route shape is structural (`<PreparedApi::regional route>/<runtime-segment>/…`).

## Evidence

- `crates/concord-core/src/request/runtime_endpoint/tests.rs`: construction typestate, symbolic inertness (no encode/iterate/body/limiter/transport at build, bind or plan), encode-once per preparation, head-limit iteration stop, first-error latch without retention, structural rules at construction and bind, replacement parity with static owner programs, suppressed owner encoders, sticky sensitivity, authentication targets/removal/challenge overlap/HTTPS, generated-ID collision, limiter presence, rate duplicates, owner projection with plan/send agreement.
- `crates/concord/tests/runtime/runtime_endpoint.rs`: the same contract through generated owners, including runtime/generated replacement parity (absent/empty/each), TLS loopback plan/send agreement with private authentication, forbidden origin/private-source/rate removal, and body/response limits.
