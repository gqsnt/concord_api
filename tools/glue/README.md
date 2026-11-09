# tools/glue

Static counters of hand-written SDK code that restates what Concord could know: forwarders,
wrapper structs, repeated facts, hand-maintained redaction. They read source text only (no
build) and are heuristics, kept from the desirability loop as a baseline for removing glue
(`v4/PRODUCT.md`, "Where the promise breaks today: glue").

```sh
python3 -I tools/glue/scoreboard.py --repo . --out scoreboard.json --against old.json
```

| Metric | Counts (lower is better unless noted) |
|---|---|
| D1a | public SDK functions outside the declaration that call a generated endpoint and prepare, send or receive it |
| D1b | public SDK structs with a field of a generated client type |
| D1c | generated share of the public request surface (higher is better; needs rustdoc JSON) |
| D2a | version literals in endpoint paths; shared path heads and query keys repeated across endpoints |
| D3 | lines in `error.rs`, `SendWith` impls, forwarders, redacting `Debug`/`Display` impls and value+`ResponseMeta` wrappers (each line once) |
| S2 | hand-written `Debug`/`Display` impls whose body withholds or redacts |
| R4a | declaration lines per endpoint |
| decl_doc_mech | declaration doc lines that explain Concord mechanics rather than the provider |
| R4c | generated rustdoc words per endpoint (needs `--rustdoc-dir` with nightly rustdoc JSON) |

Files: `scoreboard.py` (entry point and Markdown diff), `static_scan.py` (source metrics), `classify.py` (glue classifier, below), `measure.py` (spike measurements M1, M2, M4, M5, M6),
`decl.py` (`api!` declaration parser), `rustdoc_metrics.py` (D1c and R4c from rustdoc JSON).

Limits: pattern matches over source, not types. A forwarder that adds validation counts as a
forwarder; redaction written through a helper may be missed; a declaration the parser does not
recognize is skipped. Treat the numbers as trends between commits, not exact counts.

## Glue classifier (`classify.py`)

```sh
python3 -I tools/glue/classify.py --repo . --json glue.json --md GLUE.md
python3 -I tools/glue/classify.py --repo . --sdk slack-sdk          # one SDK, Markdown to stdout
python3 -I tools/glue/classify.py --repo . --md /dev/null --sample 60 --seed 7 --sample-out sample.md
```

It labels every non-blank line of `examples/*-sdk/src/**/*.rs`:

| Label | Meaning |
|---|---|
| `model` | provider data types, their serde, their accessors |
| `behavior` | decisions: auth, signing, retry, rate limiting, traversal, validation, wire formats the SDK implements |
| `glue:error` | SDK error types, `From<concord::Error>`, error `Display`, status and code mapping |
| `glue:result` | value-plus-metadata wrappers (`Response<T>`, `ResponseMeta`) and code that builds them |
| `glue:operation` | which operations are reads, writes, idempotent or paged (`is_write`, `Operation::X` arms, method checks) |
| `glue:redaction` | hand-written `Debug`/`Display` that withholds fields |
| `glue:forwarding` | short public functions that only call a generated endpoint and send it |
| `glue:construction` | structs that wrap the generated client, and their constructors |
| `glue:paging` | page types, cursor plumbing, `_from` twins |
| `glue:docs` | doc lines that restate a declared route (`GET /v1/...`) |
| `other` | imports, re-exports, module docs, constants, macros |
| `declaration` / `test` | the `api!` block and `#[cfg(test)]` modules, counted apart |

How it decides: Rust items (struct, enum, impl, fn, const, mod) are found by brace matching on the
source with strings and comments blanked; comments and attributes belong to the item below them; a
line takes the label of the innermost item. Types are labelled first (error names, result names,
wrapper fields, serde derives, file names such as `error.rs`, `pages.rs`, `models.rs`, `auth.rs`),
then trait impls by trait, then each method of an inherent impl by its body (constructor of a
wrapper, short endpoint forwarder, paging tokens, validating constructor). Line overrides: operation
arms and tables, method-name checks, doc lines that restate routes.

Known errors (from hand samples of 60 lines; the independent review measures the rate):
- File names carry weight: a model type defined in `lib.rs` without serde, or validation in
  `context.rs`, can be labelled by its owner instead of by what it does.
- Error types are glue as a whole, including the parts that encode provider semantics (retry-after
  parsing, error-code tables); the line between "restating the error document" and "deciding" is
  not visible to a pattern.
- A forwarder that adds validation of a few lines still counts as forwarding; a long one counts as
  behavior.
- Operation classification written as arbitrary logic (not arms, tables or method checks) stays in
  `behavior`, so `glue:operation` is a lower bound.

### Measured accuracy (independent review, Codex `gpt-6.1-sol`)

| Round | Seed | Top-level error | Glue sub-category error | Notes |
|---|---|---|---|---|
| 1 (v1) | 20261109 | 10/60 (16.7%) | 2/60 | owner and file fallbacks overrode method roles; policies labelled wholesale |
| 2 (after one fix round) | 20261110 | 9/60 (15.0%) | 0/60 | errors both ways: 5 lines should be glue, 3 labelled glue should not be |

The plan allows one fix round; the remaining rate is a recorded limit. Consequences: the absolute
glue total carries roughly ±15% per-line uncertainty in an SDK-balanced sample (not a LOC-weighted
estimate). Spike measurements use the **difference** before → after on the same classifier, which
cancels most systematic errors; an idea's glue removal is also checked by reading the removed code.
Open causes (review round 2): active readers with metadata fields labelled `glue:result`; derived
unit-struct policies (`#[derive(Default)] struct KeepHead;`) left as model; file-name fallbacks
(`auth.rs`, `stream.rs`, `error.rs`, paging files) still override item roles; page adapters with
ordinary names (`range`, `changes`) fall through to behavior; a formatter that prints only
`Type { .. }` is not recognized as redaction.
