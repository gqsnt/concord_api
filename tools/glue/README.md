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

Files: `scoreboard.py` (entry point and Markdown diff), `static_scan.py` (source metrics),
`decl.py` (`api!` declaration parser), `rustdoc_metrics.py` (D1c and R4c from rustdoc JSON).

Limits: pattern matches over source, not types. A forwarder that adds validation counts as a
forwarder; redaction written through a helper may be missed; a declaration the parser does not
recognize is skipped. Treat the numbers as trends between commits, not exact counts.
