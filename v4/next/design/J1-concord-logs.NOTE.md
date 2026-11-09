J1: one static head event; J3 native Summary included; J4 value-bearing spans excluded.
Manual glue removed (nonblank code): github-sdk 0, github-app 0, amazon-s3-sdk 12 (Debug 17→5), nager-holidays-sdk 0; classifier calls that S3 block “other”.
New concepts: opt-in Cargo feature `tracing`, fixed `concord.call` event schema, borrowed `Summary` with four accessors; no DSL syntax.
C1: declarations are byte-for-byte unchanged; logging adds no non-provider word.
C2: Concord renders known facts; caller owns subscriber/output; signing and workflow reports remain Rust.
C3: Nager declaration, constructor, manifest, policy and existing tests unchanged; separate additive caller only.
C4: static authored route, no filled target; dynamic query pairs withheld whole; native summaries withhold all URL/header data.
C5: no new declaration diagnostics or generated names reach authors; existing checks remain.
C6: one event per execution, one redacted view; short proposed docs are in DESIGN.md.
C7: reqwest client, prepare, send_raw and into_parts survive; decoder summary remains safe after native edits.
C8: feature off by default; interest before clock; one execution, no retries, middleware, body buffering or global subscriber.
C9: raw-subscriber canary contracts cover seven cases, wire positive controls, SigV4 verification and native secrets; unrun by instruction.
Simple control changed? No production change; its new caller demonstrates the opt-in capability.
Declaration reads better? No: it is identical; the gain is safe observation and less S3 formatting, not a clearer DSL.
No existing trio/app test changes; folded J3 flips ClickHouse's dynamic-query Prepared-Debug assertion; details and exact expected events are in DESIGN.md/EVENTS.md.
