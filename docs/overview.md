# Concord v2 overview

Concord is for Rust applications that want an HTTP API to read like its domain
while retaining one bounded operational authority. An API author declares one
client tree. Concord projects that tree into generated navigation, inert unbound
intents, request plans, static contract descriptions, and managed execution.

## What an author declares

A declaration answers questions a reviewer otherwise reconstructs across client
code and middleware:

- which values belong to the client, each lexical scope, and an endpoint;
- whether a value is required, optional, or defaulted;
- where each public value shapes subdomain, path, query, or headers;
- which body input and response output the caller sees;
- which credential source is placed where and whether one recovery is allowed;
- which stable endpoint/rate identity, repeatability, timeout, pagination, and
  terminal capabilities apply.

Rust type spellings are opaque. `?` controls one outer presence layer, `each`
controls repeated query emission, and `using Spec` selects body/response
behavior.

## What a caller receives

The generated API is the declaration tree: a client builder, immutable scope
snapshots, domain-named endpoint methods, exact typed controls, and only the
terminals an endpoint supports. `Client::unbound()` creates the same navigation
without runtime state so intent can cross application boundaries and be planned
or bound later.

Request construction and planning are inert. Await begins effects. Direct and
unbound-bound forms share endpoint identity, shaping, credentials, rate policy,
deadline, capabilities, and executor behavior.

## What the runtime guarantees

One executor owns credential-free public shaping, gates, body lifecycle,
credential isolation, rate admission, dispatch, retry/recovery, bounded response
acquisition, conversion, pagination, errors, and completion evidence. Every
growing dimension is finite.

Plans are credential-free and propagate declared redaction. They are not a
claim that visible application data is harmless. Public error context reports
dispatch knowledge rather than pretending to know whether a remote side effect
committed.

## Deliberate limits

Concord does not expose native request/response mutation, arbitrary Rust
expressions in the DSL, general middleware, a dependency container, or endpoint
syntax for transport/retry algorithms. Runtime-defined endpoints are dynamic
contracts using the same core structures, not an escape from them.

Start with the [quick start](quick_start.md), then use the normative
[DSL specification](spec.md) and [runtime model](runtime.md).
