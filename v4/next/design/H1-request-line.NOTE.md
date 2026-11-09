Hand-counted substantive doc glue removed: fred 8 lines, clickhouse 0, nager-holidays 6; including bare /// separators: 9/0/7.
Inventory-labelled glue removed: 0/0/0; fred/Nager removals are classified as declaration comments; ClickHouse's SDK-local header notes stay.
New concepts: none in declarations/calls; one shared Requests section, concise doc notation, and a per-endpoint CI word gate.
C1: no declaration syntax or provider facts added; executable declaration tokens are identical.
C2: policies, custom formats, wrappers and paging remain Rust; no behavior inferred by the doc renderer.
C3: simple-control declaration and calls keep their shape; no endpoints or runtime concepts added.
C4: prepared URL/query order and sensitivity are explicit; arbitrary Rust constants remain symbolic.
C5: generated signatures, bounds, spans and diagnostics stay unchanged; no new author validation rule.
C6: generated navigation + request type + send total 47–77 words per sampled endpoint; author prose counted separately, never truncated.
C7: caller transport, prepare(), into_parts() and send_raw() remain; shared Requests documents their semantics.
C8: only text changes; no evaluated initializers, implicit requests, retries, or hidden actors added.
C9: fixtures/source equivalence/word counts checked; future oracle, canary and CI doc-test changes listed; no compilation or SDK tests run.
Simple control changed: only documentation (6 prose lines plus a separator removed); KeepHead and holiday behavior are identical.
Does the declaration read better than before? No: its facts and shape are unchanged; fewer mechanics comments reduce noise, but H1 improves generated docs.
Coverage limits: Nager has one endpoint shown in three views; ClickHouse's three generated previews are private, with its three public wrapper entries retained separately.
