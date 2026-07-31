# Overview

- **Audience:** developers evaluating Concord
- **Purpose:** explanation
- **Normative references:** [DSL specification](spec.md), [runtime model](runtime.md), [design invariants](design_invariants.md)

## The problem

A large HTTP API has a domain structure even when its client library presents a
flat list of methods. Resources belong to services and versions; routing values
apply to subtrees; authentication and quotas apply at different boundaries;
endpoints have typed inputs, output forms, and local transport mappings.

A flat client usually encodes that structure in long method names and repeated
implementation code. A hand-written tree can represent the domain better, but
requires scope structs, constructors, shared ownership, path and policy
propagation, request wrappers, value controls, documentation, and tests.

Concord changes the cost of representing the domain correctly. The declaration
states the tree and the local differences; generation pays the repetitive Rust
cost.

## Two equally important surfaces

### The declaration surface

A reader should understand the operation from its signature before reading its
wire mapping:

```text
signature:
GET item_summaries(category: Option<String>, offset: u64 = 0, count: NonZeroU32 = DEFAULT)
    -> Json<Vec<ItemSummary>>
```

The signature communicates the domain action, required or optional inputs,
defaults, constraints, and response semantics. Path, query, body, policy, and
pagination clauses form a second reading depth for endpoint authors and
reviewers.

### The generated Rust surface

A caller should navigate the same domain and express only application choices:

```text
client.regional("eu")
    .item_summaries()
    .set_category("hardware")
    .paginate(PageLimit::Take(2))
    .await
```

Wire names, encoding, inherited routes, private authentication, limiter
admission, retries, page loops, and response acquisition remain centrally
implemented.

## Structured information becomes guardrails

| Contract information | Derived result |
|---|---|
| tree position | generated modules, scope snapshots, visibility, and inherited semantics |
| `Option<T>` | omission plus `set_*` and `clear_*` controls |
| declaration default | retained default plus `set_*` and `reset_*` controls |
| `#[body]` | caller input/spec separation and body lifecycle obligations |
| response specification | output type and available response terminals |
| auth placement | private mutation, target collision checks, and challenge attribution |
| pagination binding | exclusive live mutation and `.paginate(...)` capability |
| rate dimensions | stable bounded policy keys and compatibility validation |

The DSL is valuable because it preserves information, not because it merely
uses fewer characters.

## Internal cost accepted for the external result

Concord accepts:

- a real parser and semantic compiler;
- generated-code and compile-time cost;
- a restricted public expression language;
- family-specific inheritance rules;
- one managed executor with strict ordering;
- bounded semantic extension contracts instead of native request access;
- a large proof surface for generated presence, absence, lifecycle, security,
  and documentation.

These costs make the declaration and usage honest rather than cosmetic.

## Appropriate and inappropriate uses

Concord is most appropriate for large, hierarchical, policy-rich HTTP APIs
consumed from Rust. A small hand-written client or a generic request builder is
often better for a handful of unrelated endpoints. OpenAPI generation is often
better when a standardized upstream schema is authoritative. A lower-level
transport is better when arbitrary native middleware or exact wire signing is
the primary requirement.

## Next

- Build a client in the [quick start](quick_start.md).
- Learn the concepts in the [mental model](mental_model.md).
- See the model at scale in the [Riot walkthrough](riot_walkthrough.md).
