# Concord charter: what it is, what it promises, where it goes

This charter replaced the V4 product charter on 2026-11-09 (owner direction). It was derived from
the DSL and its usage across the 55 SDKs, not from the desirability loop's metrics
([summary](desirability/SUMMARY.md)). Every design decision is checked against it.

## In one sentence

**Concord compiles the facts of an HTTP API into its client.** The author describes what the API
is, once. The compiler turns that description into the client callers use, and checks that it is
consistent.

## The three ideas everything else comes from

**1. Scopes give each call its context, for free.**
A scope is a context, not just a URL prefix. A value set on a client or scope (credential, account,
project, API version, shared query) is visible to every endpoint below it, like a variable in
nested blocks. The call chain *is* the context:

```rust
api.project("demo").tasks().send()
```

By hand, every level of context costs a struct, a constructor, forwarding methods and clones. So
people flatten their clients, and the domain disappears. Concord makes nesting free, so the code
keeps the API's real shape.

**2. Each fact is written once and applies everywhere.**
Host, version, auth header, format, sensitivity and limits are each written where they are true.
Everything below inherits them. Changes are explicit (`append`, `override`, `remove`), and an
unresolved conflict is a compile error. Changing v1 to v2 is one line, and the compiler applies it
everywhere.

**3. The declaration describes; it never instructs.**
There is no control flow. The declaration reads like a spec, the same values always give the same
request bytes, and it is checked at compile time. Behavior is ordinary Rust outside the
declaration: what to do on a 429, how to refresh a token, when to retry.

## The promise, per person

| Who | Promise |
|---|---|
| Author | Write facts, get a typed SDK. Hand-written code is only models and behavior. |
| Maintainer | One provider change is one edit. The compiler finds every consequence. |
| Caller | Navigation mirrors the API. Required inputs are exactly the required values. Types say what comes back and what can fail. |
| Auditor | The declaration shows what goes on the wire and what is secret. "Secrets never print" is a tested guarantee. |
| Everyone | It is `reqwest` underneath, so you can always step out. A one-endpoint API and Kubernetes use the same language. |

## Rules (every design is checked against these)

| # | Rule | Violation looks like |
|---|---|---|
| C1 | **Every word in a declaration is a fact about the provider.** | Statements that configure Concord, or that turn off generated surface (`controls { remove …; }`). |
| C2 | **Facts go in the declaration; behavior goes in Rust, with typed access to the facts.** | A retry engine, limiter or refresh loop in the DSL (the V2/V3 bloat). The opposite violation: a provider fact restated by hand in SDK code (glue). |
| C3 | **The simple case never changes shape.** | Nager, PokeAPI or Frankfurter gaining lines or concepts because of a feature they do not use. |
| C4 | **Conventions only where they are deterministic and visible.** | Inference a reader cannot predict from the declaration. |
| C5 | **Mistakes fail where they were written,** with a message about the user's code. | Errors naming `__private`, `IntoValue` or generated types the user never wrote. |
| C6 | **If a feature needs a paragraph of documentation to understand, the design is wrong.** | Generated docs of hundreds of words per endpoint to explain composition rules. |
| C7 | **Leaving is always possible.** | Losing the caller's `reqwest::Client`, `prepare()`, `into_parts()` or `send_raw()`; privileged built-in formats. |
| C8 | **Deterministic, with no hidden actors.** | Middleware, implicit retries or global state in the default path. |
| C9 | **Guarantees are tested, not promised.** | A safety claim without a test that fails when it is broken (canary, wire, compile-fail). |

## Where the promise breaks today: glue

Concord tells the whole truth about the **request** and goes quiet **after it**. Wherever a
provider has a fact Concord cannot express, the SDK restates it by hand. That restating code is
**glue**. Hand-written SDK code should be only **models** (the provider's data types) and
**behavior** (decisions). Glue is the measured deficit of this mission.

| Provider fact the declaration cannot state | Glue it forces (static counters, [`tools/glue`](../tools/glue/README.md), at `734991e9` → `f178e63d`) |
|---|---|
| What an error response looks like | `error.rs` 11,102 → 11,123 lines; 54 `From<concord::Error>` |
| The success envelope; value plus metadata | 21 `Response<T>`-style wrappers |
| Which operations are reads, writes, uncertain or paged | policy lines 1,423 → 1,003 (H08 helped 8 SDKs) |
| What is secret inside SDK-owned types | 361 hand-written `Debug` impls, 2,954 lines; 314 `Display` impls |
| What is fixed at construction and what may change | public generated setters that break invariants ([the construction problem](next/inputs/CONSTRUCTION_PROBLEM.md)); the reverted `controls {}` patch |
| Where the page cursor lives | `_from` twins, forwarders |
| **Consequence: the generated surface is not the final SDK** | **98 forwarders, 22 wrapper structs, unchanged by the loop** |

## Direction: describe the whole contract without becoming a framework

The line that prevents another V2/V3 is **facts yes, behaviors no.**

| Fact (belongs in the declaration) | Behavior (stays in Rust, reads the fact) |
|---|---|
| Error document shape, per status class | Recovery decisions |
| Success envelope; where the value is | — |
| Operation is a read, write, idempotent, uncertain or paged | Retry, replay, traversal |
| Cursor location (query, body, header, link) | The paging loop (`concord-pagination`) |
| Rate-limit header names | The limiter |
| Auth scheme and placement | Token refresh |
| Values fixed at construction vs changeable | When to rotate |
| Sensitive values | **Logging.** Concord is the only layer that knows every secret, so it is the only layer that can log safely. |

## Current scope: behavior core does not implement

By rule C2 these are behaviors, so core Concord does not implement them; SDKs and applications
write them in ordinary Rust, reading the declared facts, optionally with the capability crates.

- **Authentication orchestration**: no credential providers, refresh, stores or challenge
  handling in core. A bearer token or API key is an ordinary (optionally sensitive) header or
  query value; `concord-auth` (pilot) offers sensitive credentials and a token cache.
- **Rate limiting**: no policies, windows, budgets or admission in core; `concord-ratelimit` is
  a pilot crate.
- **Retries**: no retry engine or replay in core; `concord-retry` (pilot) owns bounded
  application attempts when an SDK opts in.
- **Pagination engine**: no page collectors in core; `concord-pagination` provides bounded
  traversal for SDK code.
- **A managed executor**: no journal, deadlines-as-policy, observer framework, middleware stack
  or second transport configuration surface. Timeouts, redirects, proxies, TLS, default headers
  and connection pooling are configured on the `reqwest::Client` the application provides.
- **Contract export and inspection tooling.**

The seams an SDK uses for these jobs: a send policy (`SendWith`, `Sends`),
operation identity (`Operation`, `Endpoint`, inherent `operation()`, the automatic crate-private
operation inventory), prepared requests and unread responses (`Prepared`, `Received`), and the native request
and response exits.

## What "pro and solid" means

- **Glue goes to zero.** Measured automatically per SDK and category.
- **The declaration stays one fact per line.** DSL lines per endpoint do not grow, and there are
  no negation statements.
- **Generated docs are short,** under a fixed word budget. The surface explains itself.
- **Guarantees are tested:** no leaks (canary), predictable requests (`prepare()`), compile errors
  at the user's code.
- **The generated surface is stable** enough to depend on.

**What Concord refuses to become:** a runtime or middleware stack, an OpenAPI dump, a configuration
language about itself, or a library that needs documentation to make sense.
