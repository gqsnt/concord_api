Glue removed by hand (nonblank): datadog-events 23 (12 Rust + 11 docs); quickbooks-online 0; nager-holidays 0; excludes 4 removed import lines.
New concepts: borrowed value getters, client http()/base_url(), scope client(); sensitive getters are pub(crate); no DSL syntax.
C1: All three declarations are byte-for-byte unchanged; no generator controls or aliases.
C2: Rust can read typed facts; checked construction, auth refresh, error handling and traversal stay in Rust.
C3: Nager's original source, construction and holiday call are unchanged; optional examples/checks are added.
C4: Getter names match values; borrows follow the handle snapshot; collisions are errors, never renamed silently.
C5: Proposed collision fixtures require diagnostics at the user's value/child, naming both origins.
C6: Each getter has one sentence of semantics; no composition language or context-view type.
C7: Borrow the stored reqwest client/base; prepare(), into_parts() and send_raw() keep their existing meaning.
C8: Reading context performs no I/O, refresh, retry, clone or new default evaluation.
C9: Existing wire/canary assertions preserved; accessor checks, downstream privacy doctests and compiler fixtures supplied, all unrun.
Simple control changed? No original source or call shape change; generated accessors are additive.
Does the declaration read better than before? No: it is identical; Rust access improves, but Datadog still needs its site/constructor wrapper.
Scope: G1 alone removes no existing getter duplication here; the 23 lines are G8 synonyms. DESIGN.md and TESTS.md record limits and every migrated test.
