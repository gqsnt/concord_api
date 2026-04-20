# Developer View Of Concord

This document is a product and architecture reading of the repository as it exists now.

It is not a syntax reference. It is an attempt to capture what the maintainer is trying to build, how they want people to use it, and what design constraints keep appearing across the DSL, generated code, and `concord_core`.

## One-sentence thesis

Concord is trying to be a serious Rust API-client system where the DSL describes the API contract clearly, the generated client stays ergonomic and unsurprising, and the core runtime executes the operational concerns in a predictable, extensible, correctness-first way.

## What the maintainer appears to want

The maintainer does not seem to want "a macro that builds HTTP requests."

They seem to want a full API client authoring model with these properties:

1. The API shape should be readable from the DSL almost like upstream documentation.
2. The generated Rust API should feel normal: constructor arguments for required values, builder setters for optional/defaulted ones, `request(...).execute().await` for sending.
3. Operational concerns like auth, retry, cache, rate limit, pagination, and inflight coordination should be built in, not bolted on ad hoc in application code.
4. The runtime should still expose real extension points so advanced users can swap transport, rate limiting, cache, debug sinks, hooks, and custom auth behavior.
5. Static intent should be checked at compile time whenever possible; dynamic behavior should be explicit at runtime.

This is closer to "contract-first client engineering" than to "request builder convenience."

## The desired user experience

### 1. Authoring the API

The maintainer wants authors to model the upstream API in a structured way:

1. `client` defines the base identity of the API plus global policy.
2. `scope` mirrors real upstream API structure and shared behavior.
3. endpoints stay small and local.
4. policies inherit from client to scope to endpoint.

The repeated pattern in docs and examples is: mirror the provider's own documentation tree. If an upstream API has regional vs platform routing, named families, shared auth requirements, or repeated rate-limit plans, those should appear as nested `scope`s and reusable profiles, not as repeated per-endpoint boilerplate.

That means Concord is trying to make large API descriptions maintainable, not just make single endpoints shorter.

### 2. Calling the API

The intended call site is intentionally ordinary Rust:

```rust
let api = my_api::MyApi::new(...);
let value = api.request(my_api::endpoints::GetThing::new(...))
    .execute()
    .await?;
```

The maintainer appears to want users to think in terms of typed endpoints, not manually assembled requests. A request is not a map of strings; it is an endpoint value with typed fields plus a small set of per-request runtime overrides such as timeout, debug level, or cache mode.

### 3. Explicit lifecycle operations

A major theme across the repository is that lifecycle-changing behavior must be explicit.

Examples:

1. login is explicit via `acquire_auth_*`, not hidden during request preparation
2. cache bypass and refresh are explicit per-request controls
3. retry policy is explicit in profiles
4. `off`, `only`, patching, and inheritance are all spelled out in the DSL

This strongly suggests the maintainer dislikes hidden network side effects and magic fallback behavior.

## The core product philosophy

### 1. The DSL describes contract, not runtime internals

The DSL is being shaped around API contract concepts:

1. routes
2. params
3. bodies
4. response codecs and mapping
5. auth declaration and application
6. retry/cache/rate-limit policy
7. pagination

The runtime is then responsible for executing that contract.

This boundary appears repeatedly in docs and code:

1. `concord_macros` does parse -> semantic analysis -> code generation
2. `concord_core` owns execution, state, coordination, retries, auth slots, cache store integration, rate limiting, transport hooks, and so on

The maintainer seems to want generated code to be mostly a typed adapter over stable core primitives, not a second runtime.

### 2. Keep concepts separate when they answer different questions

A strong repository-wide pattern is concept separation.

Auth is the clearest example:

1. `secret` answers what sensitive inputs exist
2. `credential` answers where auth material comes from and how it lives
3. `use_auth` answers how it is applied on the wire

This is important because the maintainer is optimizing for mental clarity, not minimum token count.

The same design instinct appears elsewhere:

1. cache declaration vs request-time cache mode
2. rate-limit plan vs response policy
3. endpoint response mapping vs auth acquisition
4. contract DSL vs runtime extension points

They seem willing to keep multiple concepts if each one has a stable responsibility.

### 3. Avoid second mini-languages

Another recurring rule is: do not invent a special-purpose language when an existing part of the system already models the problem.

Examples:

1. endpoint-backed auth reuses endpoint output mapping instead of introducing auth-specific response extraction syntax
2. cache avoids a separate invalidation strategy DSL and keeps advanced behavior in store implementations
3. rate limiting keeps header parsing in response policy types instead of embedding header logic into the DSL

This is one of the clearest signals in the repo. The maintainer wants the DSL surface to stay small, canonical, and compositional.

### 4. Prefer explicit user intent over automation

The maintainer is not chasing maximum automation.

They care more about making the right behavior visible than about saving every line of code.

This explains several choices:

1. session auth is manual by default
2. cache bypass and refresh are explicit request methods
3. rate-limit `off` is narrowly defined rather than overloaded to mean "disable every limiter effect"
4. policy inheritance is structured rather than heuristic

The likely principle is: hidden behavior is acceptable only when it is mechanically obvious and always safe.

### 5. Correctness beats convenience in operational features

The runtime ordering is very deliberate:

1. auth before cache, so cache keys can incorporate auth identity
2. fresh cache hit before inflight/rate-limit/retry/transport
3. stale cache revalidation still goes through the normal operational path
4. auth response handling can invalidate and retry independently
5. retry and rate limiting coordinate to avoid double sleeping

This is not accidental implementation detail. It looks like a core part of the product identity: Concord should not merely "support" auth/cache/rate limiting, it should make them cooperate correctly.

## How the maintainer wants the DSL to feel

### 1. Canonical and clean

The cleanup direction is clear: remove old aliases, remove deprecated syntax, and keep only the canonical form.

The desired DSL shape appears to be:

1. `vars.*`, `secret.*`, `ep.*`
2. `scope { host[...] path[...] ... }`
3. `part[...]` for composition
4. no parallel legacy vocabulary

This suggests the maintainer wants the language to feel settled: small, obvious, and current.

### 2. Close to API docs, close to Rust

The DSL is not trying to look like JSON schema or OpenAPI fragments. It reads more like structured Rust-flavored API notes:

1. scopes match documentation sections
2. endpoints look like named operations
3. params stay typed
4. policies are local and readable

Then the generated API keeps the Rust side natural:

1. typed endpoint structs
2. `new(...)` with required args
3. setters for optional/defaulted fields
4. explicit request execution

So the user story is:

1. author describes API contract in a compact DSL
2. consumers use the result like a normal typed Rust client

### 3. Inheritance should remove repetition, not create mystery

Policy inheritance is central to the system, but the maintainer seems to want it tightly controlled:

1. client sets defaults
2. scope shares route and policy context
3. endpoint is the final, most specific contract
4. features support patch, replace, or off semantics where needed

This is not loose cascading. It is a controlled inheritance model for API-family reuse.

That matters because the system is clearly meant to scale to large APIs, not just demos.

## How the maintainer wants the runtime to feel

### 1. The runtime is a state machine, not a bag of helpers

`concord_core` is not just utilities. It is an execution engine with a carefully ordered pipeline and shared runtime state.

Important shared state includes:

1. auth state
2. cache store
3. rate limiter
4. inflight registry
5. retry policy
6. debug/runtime hooks

Generated clients wrap that engine. Clones share the operational state that should be shared.

This strongly suggests the maintainer wants cloned clients to represent multiple handles to one logical API runtime, not isolated copies.

### 2. Advanced behavior belongs in core traits

The repository repeatedly uses traits as the escape hatch for advanced cases:

1. `Transport`
2. `CacheStore`
3. `RateLimiter`
4. `RetryPolicy`
5. `RuntimeHooks`
6. `CredentialProvider`
7. `AuthUsage`

The intended model seems to be:

1. common teams stay in the generated DSL wrapper
2. advanced integrators drop down into `concord_core`
3. the DSL does not have to encode every environment-specific need

That is a healthy boundary. It prevents DSL bloat while keeping the system extensible.

### 3. Shared mutable runtime state should be intentional

The maintainer clearly wants these to be shared across clones:

1. acquired manual credentials
2. cleared credentials
3. cache entries
4. rate-limit state
5. inflight coordination

The product intuition here is strong: a client clone is another handle to the same API session/runtime, not a logically separate API world.

## Feature-specific product intent

### Authentication

Auth is being designed as a first-class lifecycle system, not just header injection.

The intended auth model is:

1. static credentials should remain trivial
2. provider-based credentials should support acquisition/refresh
3. endpoint-backed session credentials should reuse endpoint machinery
4. manual/session credentials should fail clearly before acquisition
5. invalidation and retry should be separable
6. internal auth requests must be possible, but recursion must be blocked

The most important signal is that manual login must not become invisible. The maintainer wants auth flows to stay explicit because they often require runtime input, user consent, or side effects.

So the auth goal is not "make auth automatic." It is "support all real auth lifecycles without collapsing the mental model."

### Caching

Cache is being shaped as a practical HTTP-aware acceleration layer with explicit operator control.

The desired cache behavior is:

1. safe defaults
2. three explicit request modes: default, bypass, refresh
3. auth identity partitioning by default
4. stale revalidation integrated with the normal send pipeline
5. custom stores supported without forcing everyone into advanced APIs

The maintainer does not appear to want a cache strategy language. They want a compact DSL plus a runtime store model that can get serious when needed.

That means cache should feel powerful without becoming a separate subsystem users have to learn from scratch.

### Rate limiting

Rate limiting is being treated as request planning plus runtime enforcement, not just sleep-after-429 logic.

The intended shape is:

1. DSL declares reusable plans and keys
2. runtime acquires permits before transport
3. response policy can observe upstream-specific limit headers and cooldowns
4. retry/cache/inflight coordination must remain correct
5. runtime memory and duplicate-plan amplification must be controlled

This shows the maintainer cares about operational realism. The rate limiter is supposed to hold up for real APIs with multiple buckets, host/endpoint scoping, cooldown memory, and non-standard response headers.

### Retry

Retry is intentionally modest and policy-driven.

The maintainer seems to want:

1. reusable profiles
2. clear triggers
3. idempotency awareness
4. coordination with rate limiting and cache
5. small budgets by default

Retry is not supposed to become a scripting environment. It exists to encode safe, well-bounded resilience behavior.

### Routing and values

Routing is not just string concatenation.

The intended model is:

1. host and path are distinct concepts
2. dynamic values are typed and formatted once
3. `part[...]` composes a single route/header/query value from multiple pieces
4. dynamic path values stay percent-encoded as one segment

This means the maintainer wants correctness and structure to replace manual URL formatting.

## What the maintainer likely does not want

Based on the direction of changes, the maintainer likely does not want:

1. multiple spellings for the same concept
2. aliases for earlier internal experiments
3. hidden network calls during normal request preparation
4. DSL features that duplicate existing endpoint/runtime capabilities
5. giant generic configuration surfaces with weak semantics
6. runtime-only failure for facts known entirely at macro time
7. feature interactions that are individually correct but systemically wrong

The codebase is moving toward one canonical model per concern.

## Architecture the maintainer is converging on

### 1. `concord_macros`: validate and lower the contract

The macro crate should:

1. parse a small canonical language
2. run semantic validation
3. compute a stable IR
4. generate wrappers around core primitives

It should not accumulate alternate syntax logic forever. Its job is to define and lower the contract cleanly.

### 2. `concord_core`: execute the runtime graph

The core crate should:

1. own operational state
2. run the request pipeline
3. expose extension traits
4. coordinate auth/cache/retry/rate-limit/inflight correctly
5. remain usable directly outside the macro

So the core is the execution engine; the DSL is the contract authoring surface.

### 3. Generated code: ergonomic, boring, explicit

Generated code should not feel magical.

It should:

1. expose normal constructors and setters
2. forward into the core client
3. surface explicit helpers for meaningful lifecycle transitions
4. share runtime state in the expected places

The generated layer should make the core pleasant to use, not hide what kind of system it is.

## The clearest guiding principles

If this repository had to be reduced to a few rules, they would likely be these:

1. One concept, one canonical spelling.
2. Prefer explicit lifecycle and state transitions over hidden automation.
3. Reuse existing system pieces instead of inventing feature-specific sublanguages.
4. Do compile-time validation for static facts and runtime handling for dynamic facts.
5. Make feature interactions correct before making them clever.
6. Keep the generated API ordinary and typed.
7. Keep the core extensible without forcing that complexity into the DSL.

## Practical interpretation for future work

If future changes are consistent with the current direction, they should probably be judged by these questions:

1. Does this make the DSL more canonical or more fragmented?
2. Does this preserve clear concept boundaries?
3. Does this reuse an existing mechanism instead of adding a parallel one?
4. Does this keep hidden runtime behavior to a minimum?
5. Does this improve interaction correctness with auth/cache/retry/rate-limit/inflight?
6. Does this belong in the DSL, in generated wrappers, or in `concord_core` traits?
7. Will the resulting client still feel like normal Rust code using typed endpoints?

## Bottom line

The maintainer appears to be building a contract-first Rust API client platform for real APIs, not a macro trick.

The desired end state is:

1. a small, stable, canonical DSL
2. generated clients that are pleasant and unsurprising to call
3. a core runtime that handles serious operational concerns correctly
4. explicit lifecycle behavior where hidden magic would be unsafe
5. extension points in the core instead of complexity explosion in the DSL

That is the clearest through-line across the documentation, examples, tests, and recent cleanup work.
