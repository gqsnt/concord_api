Glue removed (hand-counted nonblank lines): Strava 10 net (12 old lines → 2 bindings); Kraken Spot REST 0; PokeAPI 0.
Separately N3 removes 3/2/0 declaration ceremony lines; Kraken adds one `#[read]` line; net `src` reductions are 13/1/0.
New concepts: endpoint/scope `#[read]`, `Operation::is_read()`; automatic `<Client>Operation` and a SendWith adapter replace `operations Name;`.
C1: `#[read]` states provider effect; the generated-enum opt-in disappears.
C2: effect is generated; retries and uncertainty decisions stay Rust; signing, quota and permission classification remain.
C3: PokeAPI's source and caller shape are unchanged; its generated surface gains an unused enum.
C4: GET/HEAD/OPTIONS read, other methods non-read; the visible Kraken scope overrides POST; PUT never implies replay.
C5: redundant `#[read]` and automatic-name collisions must diagnose authored locations; these diagnostics are specified, not implemented.
C6: “Reads follow the method unless `#[read]` says otherwise”; enum naming needs no declaration sentence.
C7: caller transport, unsigned `prepare()`, `into_parts()` and `send_raw()` remain available.
C8: facts add no actor or request edits; Kraken signing/nonces and Strava retry counts remain unchanged.
C9: complete design acceptance tests plus the required carrier/cfg/diagnostic matrix are supplied; no compilation or test execution.
Simple control changed? No authored SDK change: PokeAPI `src/lib.rs` is byte-identical; new caller/test files demonstrate the contract.
Does the declaration read better than before? Yes: Kraken says its signed POSTs are reads once; Strava loses machinery; PokeAPI stays the same.
Existing SDK tests needing changes: none. Library test migrations, compatibility costs and the hand-count ledger are in TESTS.md and DESIGN.md.
