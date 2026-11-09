I3 variant (a): exact inputs except written String/HeaderValue; baseline ba10189b; design only, no compilation.
Glue removed, hand-counted from complete source files: cloudflare-zones 0; renamed 0; nager-holidays 0; no modules shrink/disappear.
New concepts: no DSL syntax; one lexical convenience rule (aliases are exact); IntoValue remains for migration.
C1: declarations retain provider facts; no compiler controls added.
C2: models, validation, envelopes, errors and paging remain in Rust; inference removes no SDK glue.
C3: Nager source and literal-country caller shape are unchanged.
C4: deterministic final-name exception, but alias-only refactors change borrowed acceptance; real compatibility cost.
C5: exact custom inputs require expected/found errors at user code; String/HeaderValue still risk IntoValue/E0283 diagnostics.
C6: one sentence specifies input acceptance; no per-endpoint composition documentation.
C7: native Client, prepare(), into_parts(), send_raw() and custom formats remain available.
C8: no execution, retry, initializer-timing or snapshot behavior changes.
C9: existing assertions retained; required pass/fail and wire/canary checks in TESTS.md are unrun design obligations.
Simple control changed: no; its complete source is identical, and holidays("AT", 2026) remains valid.
Does the declaration read better than before? No: two are identical; renamed needs .clone(); only signatures/caller inference improve.
BREAKAGE.md enumerates three calls + two initializers in active source and 21 conditional historical expressions separately.
