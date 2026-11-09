A2 only: complete overlays and callers under `<sdk>/examples/<sdk>-sdk/`; declarations are unchanged.
Glue removed, hand-counted net nonblank SDK lines: salesforce 55; discord-rest −3 (adds 3); frankfurter 0 (COUNTS.md).
New concepts used: `derive(concord::Error)`, `display` string/formatter hook, `from(skip)`; no new DSL words.
C1: no SDK error type or generator controls are added to the declaration.
C2: only mechanical Rust plumbing derives; decoding, classification, recovery and custom accessors remain Rust.
C3: Frankfurter's declaration and error module remain byte-identical; adoption is optional.
C4: tuple conversions and Box allocation are explicit conventions; named fields and context are never guessed.
C5: duplicate conversion sources and invalid formatting/hooks must fail at the authored attribute or variant.
C6: two helper attribute names used; Discord's growth exposes the cost of preserving custom formatting.
C7: caller client, prepare/into_parts/send_raw and unread native errors remain available.
C8: derive performs no I/O or retries; existing bounded reads occur only through existing explicit paths.
C9: source comparison checked; unchanged SDK suites plus additional canary/compile-fail obligations are in TESTS.md, unexecuted.
Simple control changed? No; its existing source chains also prevent an equivalent A2 error-impl replacement.
Existing tests needing changes: none; every variant, field, formatter branch and classification is retained.
Does the declaration read better than before? No: it is identical; only Salesforce's Rust plumbing becomes shorter.
