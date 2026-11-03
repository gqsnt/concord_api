# V3 baseline build (comparison data only — does not certify V4)

- Source: `/home/canarit/projects/concord_api_v3ref` @ `a492d3678e8330307cd325a05ca0ee72499e9726` (tag `concord-v3-preserved`), unmodified.
- Run: 2026-11-01T17:06:11+01:00 → 17:06:59+01:00, rustc 1.97.1, i5-13600K (20 threads), Linux 7.0.0-42.
- Target dir: fresh, empty `CARGO_TARGET_DIR` in the session scratchpad; crates from the local registry cache (`--offline`). The global cargo registry was warm (sources already extracted), so the timing excludes download/extraction.

| Command | Result | Wall time | Peak RSS |
|---|---|---|---|
| `cargo build --offline` (default members: concord, concord-core, concord-extras, concord-macros, concord-examples, 2 downstream crates; dev profile) | success | 24.30 s | 1.64 GB |
| `cargo test --offline -p concord-core -p concord-macros` | 827 passed, 0 failed, 0 ignored across 11 test binaries | 23.72 s (includes test-profile compilation) | — |

Single run, no repetition; use only as an order-of-magnitude comparison for V4 compile cost.
