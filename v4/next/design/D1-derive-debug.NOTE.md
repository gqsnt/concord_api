# D1-derive-debug
Salesforce: 126 handwritten glue lines removed; 100 net nonblank src lines removed (hand ledger: DESIGN.md).
Twilio: 27 handwritten glue lines removed; 19 net src lines removed; D4 variant saves 20 net.
PokeAPI: 0 glue/source lines removed; its complete declaration/module is byte-identical; one caller added.
New concepts: derive(concord::Debug), field/type/variant #[sensitive]; optional D4 adds diagnostic #[public].
C1: api! facts are unchanged; Rust sensitivity attributes describe the confidentiality of stored values.
C2: secrecy facts move onto model fields; computed summaries, validation, Display and all behavior stay Rust.
C3: the simple control keeps its existing source and concepts; its new example uses ordinary Debug.
C4: explicit markers only; unmarked fields use std formatting; written Option preserves presence; D1 fails open on new fields.
C5: required derive errors point to the authored field/attribute; compile-fail coverage remains a Concord follow-up.
C6: D1 has four short rules; D4 adds a second default and a potentially confusing #[public] word.
C7: public data access, caller reqwest client, prepare(), into_parts() and send_raw() remain available.
C8: formatting adds no I/O, retries or global state and never calls withheld payload Debug.
C9: existing wire/canary assertions retained; proposed poison/pretty/public-diagnostic tests supplied; no compile/tests run.
Does the declaration read better? No: api! is identical; Rust secrecy is clearer locally but marker-heavy; D4 is terser.
