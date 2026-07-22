# Testing, documentation, and contributor tooling

> Audience: Concord contributors

The repository uses unit tests, deterministic runtime tests, Trybuild, isolated Cargo fixtures, examples, Rustdoc, and benchmark smoke tests. The normal contributor surface is intentionally small.

## Testing strategy

Concord combines unit, integration, UI, standalone Cargo, local protocol, documentation, feature, and benchmark tests. Each layer proves a different property.

### Test layers

#### Unit tests

Unit tests live beside implementation modules and prove deterministic state transitions, arithmetic, parser normalization, error categorization, finite bounds, and ownership rules.

#### Integration tests

Crate integration tests prove the stable public surface and full managed lifecycle. Important groups include:

- body and codec extensions;
- generic request/response specifications;
- authentication and provider HTTP;
- rate limiting and pagination;
- records and streaming;
- raw response and dangerous features;
- public Rustdoc and redaction.

#### Trybuild UI tests

`tests/ui/fail` contains compile-fail source files and their expected `.stderr`. Use Trybuild for:

- privacy boundaries;
- absent feature surfaces;
- malformed DSL;
- forbidden method availability;
- trait-bound failures that should remain source-focused.

Do not leave generated `.stderr` in `wip/` once accepted.

#### Standalone Cargo fixtures

Standalone packages are justified only when the proof depends on the package graph, manifest metadata, renamed dependencies, missing dependencies, or feature unification.

They run locked and offline with isolated target directories.

#### Local HTTP tests

Use ephemeral loopback servers, deterministic scripted responses, barriers, counters, and controlled Tokio time. Do not depend on external services or arbitrary sleeps.

#### Rustdoc and examples

- Public Rustdoc tests inspect rendered output for stable paths and privacy.
- Generated Rustdoc fixtures inspect generated endpoint documentation.
- `concord_examples` owns complete runnable examples.
- Documentation pages should link to examples rather than duplicate long programs.

#### Benchmarks

The `perf` crate provides bounded smoke benchmarks for critical preparation and runtime operations. Benchmarks exclude external network latency and do not impose performance thresholds unless a requirement explicitly defines one.

### Commands

The simplified contributor commands are:

```text
just fmt
just check
just check-all
just test
just test-all
just lint
just docs
just examples
just features
python scripts/check.py
just verify
```

`cargo nextest` runs unit and integration tests. Doctests still use `cargo test --doc`.

### Test quality rules

A strong test:

- selects the exact behavior it claims;
- has a positive oracle rather than scheduler timing;
- includes a negative control where ambiguity is possible;
- checks no unexpected side effect occurred;
- uses sentinel values for redaction boundaries;
- records the first-excess behavior, not only the error type;
- avoids test retries that hide nondeterminism;
- preserves feature isolation.

### Adding a test fixture

Before creating another standalone directory, ask whether an ordinary unit, integration, or Trybuild test proves the same behavior. Empty placeholder fixture directories should not be committed.

## Documentation conventions

The canonical documentation is portable Markdown. It must remain useful on GitHub and portable across GitHub and static documentation renderers.

### Page structure

Documentation sections use a landing `README.md`; substantive pages are normal Markdown files beside it.

User-facing leaf pages should normally contain:

```text
Audience / feature / stability metadata
What this solves
When to use it
Quick example
How it works
Available options
Errors and limits
Security and redaction
What it cannot do
Alternatives and extension path
Related pages
```

Section landing pages are short reading guides and should not duplicate the substantive pages beside them.

### Truth ownership

- Manual docs own concepts, workflows, behavior, examples, and limitations.
- Rustdoc owns exact signatures and trait bounds.
- `docs/reference/dsl.md` owns exact macro syntax.
- `docs/reference/features-limits-and-errors.md` owns feature topology.
- `docs/reference/features-limits-and-errors.md` owns numeric defaults.
- `concord_examples/examples/` owns complete runnable programs.

Link to the canonical owner instead of copying exact facts into several pages.

### Code examples

Short excerpts may live in Markdown. Long examples must link to a runnable file in `concord_examples`.

Examples must:

- use current DSL syntax;
- list required features;
- avoid real secrets;
- avoid external network in tests;
- compile under their documented configuration;
- avoid internal or generated-private imports.

### Unsupported behavior

Every capability page must state meaningful unsupported behavior. The central source is [Unsupported behavior and alternatives](../concepts/security-bounds-and-limitations.md#unsupported-behavior-and-alternatives).

Do not imply support for:

- native response access;
- custom transports;
- prepared pagination;
- distributed strict quotas;
- unbounded I/O;

### Mermaid

Use Mermaid when it clarifies a lifecycle, state machine, dependency graph, or ownership boundary. Follow each diagram with a prose explanation so the page remains understandable without Mermaid rendering.

### Terminology

Use current names consistently:

- `RequestBodySpec` and `ResponseBodySpec`;
- `ApiResponse<T>`;
- `ByteStream`;
- `RecordStream<D>`;
- typed pagination controller;
- lexical rate limit;
- advanced explicit rate-limit key.

The documentation checker rejects obsolete public terminology and internal governance language in user pages.

### Validation

Run:

```text
just verify
just examples
just docs
```

before merging documentation changes.

## Version and compatibility

The workspace version, Rust version, edition, pinned dependencies, and generated contract are defined in Cargo manifests and source constants. A generated-contract bump is required only when the private core/macro bridge changes; it is separate from the package version.

Keep source archives reproducible: retain the workspace lockfile because standalone fixture projects use locked, offline Cargo checks, and keep `deny.toml` aligned with the actual dependency graph.

## Normal contributor commands

```text
just fmt
just check
just test
just test-all
just lint
just docs
just examples
just features
just verify
```

`concord_test_support` remains a workspace crate because deterministic executors, loopback servers, TLS, gates, and Cargo-fixture helpers are shared by both product crates. The standalone fixture workspace is reserved for behavior that genuinely depends on Cargo metadata or feature isolation.
