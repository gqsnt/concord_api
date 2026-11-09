Hand-counted net test glue removed: github 49 lines; amazon-s3 12; frankfurter 41 (COUNT.md details).
Production src/ glue removed: 0 each; fixture implementation is relocated, not erased or counted as SDK savings.
New concepts: published concord-test; named Shape/Expected scenarios + conformance driver; named/public/derived canary probes; strict/wire/origin-script helpers.
C1: no declaration change; every existing declaration word remains a provider fact.
C2: test behavior stays Rust; provider-specific mapping, redirect policy and SigV4 verifier stay in SDK tests.
C3: Frankfurter's declaration, constructor and caller shape are unchanged; its dev-dependency and tests change.
C4: seven explicit shapes and kind/delivery expectations; wire snapshots show built requests before transport defaults.
C5: assertion failures name SDK/probe/secret labels; cycles and invalid proof inputs fail locally; no compiler change.
C6: named shapes, no SDK scenario trait; the three-closure driver adds 10 lines to Frankfurter's test body and merits scrutiny.
C7: strict helpers return native reqwest types; prepare/into_parts/send_raw and S3's signing seam remain.
C8: serial, per-test owned servers; no implicit retries; the existing inert refused socket is the only process-wide fixture resource.
C9: preserve actual wire/negative tests; add public-field and independent derived-use controls; future crate acceptance checks are specified, not claimed run.
Simple control changed: tests/dev-dependency only; no production module, declaration or production caller change.
Does the declaration read better than before? No: it is byte-for-byte unchanged; the improvement is less copied test infrastructure.
Existing test outcomes need no change; migrations/stronger assertions are listed in TESTS.md. Design only: syntax reviewed, never compiled.
