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

## Gate 5 — hardening (final run: see bottom)

Command: `scripts/verify.sh --msrv` (fmt, clippy `-D warnings` on all targets, workspace tests,
three feature combinations, rustdoc `-D warnings`, MSRV suites).

| Check | Command | Result |
|---|---|---|
| Format | `cargo fmt --all -- --check` | filled at final run |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings` | |
| Tests (default features) | `cargo test --workspace` | |
| No default features | `cargo test -p concord --no-default-features` (JSON suites `slice`, `bodies` and the JSON doctest are skipped via `required-features`/`cfg`, not counted as passed) | |
| `json` only | `cargo test -p concord --no-default-features --features json` | |
| `reqwest-defaults` only | `cargo test -p concord --no-default-features --features reqwest-defaults` | |
| Rustdoc | `RUSTDOCFLAGS=-D warnings cargo doc -p concord --no-deps` | |
| MSRV 1.85.1 | runtime suites + examples (`--lib --test slice/values/composition/bodies/sensitivity/hygiene`) | |
| MSRV UI | `cargo +1.85 test -p concord --test ui` | 32/37 identical; 5 differ only in rustc's own wording (`missing_required_argument`, `newtype_is_not_converted`, `option_has_no_wire_spelling`, `required_has_no_setter`, `vec_needs_each`); every case still fails to compile |
| Windows compile check | `cargo check --target x86_64-pc-windows-gnu -p concord --no-default-features --features json --tests` | type-checks; examples with default features not checkable (aws-lc-sys needs a MinGW C compiler). **No Windows execution.** |
| Scope scan (L06) | `grep -rniE 'retr(y|ies)|rate.?limit|throttl|paginat|cursor|credential|refresh|oauth|bearer|token|backoff|journal|operation.?id|middleware|intercept|hook' crates/*/src` | only documentation of non-goals, syn token parsing and the redaction name list (`refresh-token` is a name to redact) |
