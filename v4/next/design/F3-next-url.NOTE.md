Glue removed (manual nonblank ledger, COUNTS.md): github 133; digitalocean 23; pokeapi 0 net lines.
Attribution: GitHub includes 138 from sharing relation/error projection; DigitalOcean includes 22 item-projection lines shared with F2.
New concepts: page next link / next url + items; opaque NextUrl; Follow/Followed and typed LinkContinuation/UrlPage projections; shared link facade.
C1: clauses state provider continuation sources and item fields; no controls or Concord configuration in the declaration.
C2: dispatch/projection generated; trust, binding, repeat rejection, caps and traversal stay Rust; GitHub's explicit KeepMeta wraps its existing boundary.
C3: PokeAPI source and declaration are byte-identical; the added caller example uses its existing manual follow.
C4: opt-in sources and Rust field paths are visible; DigitalOcean retains exact-path/tag checks; GitHub retains repository-id/rename bindings.
C5: invalid source/path/type/method must fail at declaration tokens; compile-fail acceptance cases are specified in TESTS.md.
C6: one caller sentence explains follow; existing callers keep pages/items; Rust author projection traits remain a real complexity cost.
C7: caller transport, prepare/into_parts/send_raw and Prepared::follow remain; custom formats use the same Rust projections.
C8: one request per page through the selected SDK policy; no construction I/O, implicit retries, tasks or global state.
C9: existing tests retain assertions; supplied policy/identity/redaction regressions and core acceptance cases are unrun by instruction.
Tests needing change: none; custom downstream LinkPages::new calls drop the prepare argument. Simple control changed: no SDK change.
Does the declaration read better than before? Yes: next-page sources are visible at each listing; the win is modest and binding glue remains.
