Glue removed, hand-counted code lines (imports/docs/tests excluded): Salesforce −15 (10 saved, 25 FromStr adapters added); distribution-registry 0; pokeapi 0. Negative means growth; see DESIGN.md tally.
New concepts: E::from_url, UrlMismatch/UrlRule, conditional FromStr inverse, required non-URL constructor arguments; no DSL syntax or generated getters.
C1: All three declarations are unchanged; no Concord configuration words enter the DSL.
C2: Partial: declared routes replace manual URL matching; compatibility, typed extraction and error adaptation still cost glue.
C3: PokeAPI library/declaration and its compatibility surface stay byte-for-byte unchanged.
C4: Relative-base resolution, exact client values, canonical spellings and query reordering are explicit; no inference of opaque URLs.
C5: Errors identify rules/declared names only; missing-FromStr UI fixture specifies the call-site diagnostic; implementation verification remains pending.
C6: Generated documentation stub is under 60 words; inverse eligibility/non-URL arguments qualify the idea's literal two-argument promise.
C7: Caller reqwest client, prepare, into_parts, send_raw and Prepared::follow remain available.
C8: Recognition performs no I/O, retries, refresh or traversal; existing Rust policies remain in charge.
C9: Round-trip, mismatch/parser canaries and compile-fail fixture authored; unexecuted because this task forbids implementation/compilation.
Simple control changed? No library/declaration change; only the requested example and an opt-in acceptance test are added.
Does the declaration read better than before? No: it is identical; canonical continuation calls improve, but isolated G2 grows total charged glue here.
Existing tests needing changes: none; compatibility paths retain exact-query and opaque-Location contracts (TESTS.md).
Scope: design overlays only; no Concord implementation or repository edits; source formatted, never compiled.
