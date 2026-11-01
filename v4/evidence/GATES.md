# Gate record

All runs on the machine described in `BASELINE.md` (Linux x86_64, i5-13600K). Model: Claude Opus
5.5 (`claude-opus-5-5`, effort xhigh) as coordinator; subagents Opus 5.5 (medium/xhigh per profile);
independent reviewer Codex CLI 0.166.0, `gpt-6.1-sol`, `model_reasoning_effort=xhigh`,
`--sandbox read-only`.

## Gate 0 — baseline (2026-11-01T17:06+01:00)

See `BASELINE.md` and `v3_baseline.md`.

## Gate 1 — research and blind challenge

| Item | Result |
|---|---|
| Blind Codex design challenge (before any V4 design existed) | `reviews/codex/blind_20261101T160427Z_a492d3678e83.md`, run on V3 SHA a492d36 |
| Claude research tracks (independent, read-only) | `research/claude_{v3_auditor,semantics,composition,ergonomics}.md` |
| Decisions | `../DECISIONS.md` D1–D10 |

## Gate 2 — vertical slice (`2639b39`)

`cargo test --test slice`: 7 passed. Codex review `reviews/codex/gate2_20261101T163326Z_2639b39.md`:
7 findings, all confirmed and fixed (`reviews/TRIAGE.md`).

## Gates 3–4 — semantics, composition, bodies, sensitivity (`24717e8`, `bfb4870`)

Claude adversarial review on `24717e8`: 10 findings fixed, 1 documented. Codex Gate 3/4 review on
`bfb4870`: see `reviews/TRIAGE.md` (first attempt aborted when the /tmp volume ran out of quota;
re-run on the same snapshot).

## Gate 5 — hardening

Final run on `caf7cb3` (clean tree, `Cargo.lock` committed and unchanged by the run), 2026-11-01,
Linux x86_64, rustc 1.97.1; MSRV with rustc 1.85.1. Command: `scripts/verify.sh --msrv`
(every cargo invocation uses `--locked`; the script fails if tracked files change).

| Check | Command | Result |
|---|---|---|
| Format | `cargo fmt --all -- --check` | PASS |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` | PASS (0 warnings) |
| Tests, default features | `cargo test --workspace --locked` | PASS, 101 tests — concord: bodies 9, composition 36, hygiene 2, representations 10, sensitivity 13, slice 7, tls 2, ui 1 (41 compile-fail cases), values 13, doctests 3; heap check 1; shapes wire 3; renamed 1. 0 failed, 0 ignored |
| No default features | `cargo test --locked -p concord --no-default-features` | PASS, 78 tests. **Skipped, not passed:** `bodies`, `slice` (`required-features = ["json"]`), `tls` (`reqwest-defaults`); the JSON quick-start doctest compiles to nothing |
| `json` only | `cargo test --locked -p concord --no-default-features --features json` | PASS, 94 tests (`tls` skipped) |
| `reqwest-defaults` only | `cargo test --locked -p concord --no-default-features --features reqwest-defaults` | PASS, 80 tests (`bodies`, `slice` skipped) |
| Rustdoc | `RUSTDOCFLAGS=-D warnings cargo doc -p concord --no-deps` | PASS |
| MSRV 1.85.1, runtime suites | `cargo +1.85 test --locked -p concord --lib --test slice --test values --test composition --test bodies --test representations --test sensitivity --test hygiene --test tls` | PASS, 92 tests |
| MSRV 1.85.1, downstream | `cargo +1.85 test --locked -p concord-example-shapes -p concord-example-renamed -p concord-heap-check` | PASS, 5 tests |
| MSRV UI | `cargo +1.85 test -p concord --test ui` | 35/41 identical; 6 differ only in rustc's own wording (`conversions_are_closed`, `missing_required_argument`, `newtype_is_not_converted`, `option_has_no_wire_spelling`, `required_has_no_setter`, `vec_needs_each`); every case still fails to compile. UI expectations are pinned to 1.97.1 |
| UI stability | fresh target directory, 1.97.1, `-p concord --test ui` and `--workspace` | PASS; one audit run saw `help:` wording drift in 2 rustc-generated cases — not reproduced |
| Windows | `cargo check --target x86_64-pc-windows-gnu -p concord --no-default-features --features json --tests` (at `1039f13`) | type-checks; **no Windows execution**. Default-feature examples not checkable (aws-lc-sys needs a MinGW C compiler) |
| macOS | — | NOT_RUN |
| Scope scan (L06) | `grep -rniE 'retr(y|ies)|rate.?limit|throttl|paginat|cursor|credential|refresh|oauth|bearer|token|backoff|journal|operation.?id|middleware|intercept|hook' crates/*/src` | only documentation of non-goals, syn token parsing and the redaction name list (`refresh-token` is a name to redact) |
| Dependencies of `concord` | `cargo tree -p concord -e normal --depth 1` | bytes, concord-macros, percent-encoding, reqwest, serde + serde_json (feature `json`), url |
