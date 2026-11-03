# Modular codecs increment — status

Branch `concord-v4-modular` (worktree `/home/canarit/projects/concord_v4_modular`), started from
`concord-v4-final` at `dfdeed8` (implementation `81836ec`). Not pushed; no upstream. Work
directory (evidence, briefs, frozen reports, snapshots): `/home/canarit/projects/concord_v4_modular_work`.

## M0 — baseline (reproduced before any change)

| State | Source | Toolchain | Command | Result |
|---|---|---|---|---|
| 1. historical | `dfdeed8` | rustc 1.97.1 (pin), MSRV 1.85, nightly | `scripts/verify.sh --msrv --nightly --package` | 24/24 PASS; workspace 601 tests, no-default 384, json-only 436, reqwest-defaults 386, form-only 387, form-json 5, multipart 11, MSRV 442 + 130 + 11, nightly 16 |
| 2a. same code, new compiler | `dfdeed8` + pin 1.99.1 only | rustc 1.99.1 | `scripts/verify.sh --nightly` | clippy FAIL (6 `collapsible_if`: let chains), `ui` FAIL (3 snapshots, rustc wording only); every runtime suite passes |
| 2. control | `686cd24` (pin + required compatibility changes) | rustc 1.99.1 | `scripts/verify.sh --nightly` | 17/17 PASS; the same counts as state 1 |

The latest stable Rust was rechecked on 2026-11-03 from the official release index
(<https://blog.rust-lang.org/releases/>): 1.99.1 (2026-10-15) is the newest release. It is both
`rust-version` and the `rust-toolchain.toml` pin. The machine's default toolchain is unchanged
(stable 1.99.0); `clippy`, `rust-src` and `rust-analyzer` were added to the existing 1.99.1
toolchain.

## Slices (each passed `scripts/verify.sh` on its own head)

| Commit | Slice | Gate |
|---|---|---|
| `686cd24` | Rust 1.99.1 minimum and pin | 17/17 (no `--package` yet) |
| `93f9b0e`, `3a836e7`, `013de15`, `a4b465a` | Riot redirects (R1), sealed `NativeKey` (R3), one gate inventory (R7), occurrence owners (R5) | 20/20, 606 workspace tests (`a4b465a`) |
| `df37383` | formats are type paths | 22/22, 606 |
| `7ec1f4b`, `dc7f134` | four crates (behaviour unchanged) | all steps, 606 (`dc7f134`) |
| `ca545f0` | metadata-aware `Decode`, `CodecError`, `Accept`, strict `Json` | all steps, 630 |
| `25a52f7` | `validate_header_names()` (R2) | all steps, 636 |
| `1f26e91` | consumers in the gate, path experiment deferred (R6) | 33/33, 639 |
| `d5c93f0`, `9f7eb83` | documentation: codec contract, strict `Json`, `Accept`, format guide, validator guide, SEMANTICS | 34/34 with `--nightly`, 643 (`9f7eb83`) |
| `9f8939f` | **frozen candidate** for the final reviews (records, invalid-`Accept` fixture, an exact vendor media type) | 34/34 with `--nightly`, 644 |
| `ee0117c` | final-audit corrections X-F1/X-F2 (charset parameters) and the fresh-use notes | the 34 steps passed, 648; the closing clean-tree check failed because the coordinator edited a records file during the run (work directory evidence/verify_ee0117c-4ce3d01.txt); superseded by the next rows |
| `d98e4cf` | semantic-review corrections S1–S4, M1–M8 (RFC 9110 parameters, the sealing test, validator wording) | 34/34 with `--nightly`, 650 (`530b6c1`) |
| `3f42a32` | `Text` and vendor-example documentation after the targeted re-review | 34/34 with `--nightly`, 650 (`3fa14b7`) |
| `abdad04`, `2cbac44` | re-review corrections: `Text` refuses only a readable non-UTF-8 charset (N2); type and subtype must be tokens (RR2-1); documentation (N1, N3, RR2-2..4) | 34/34 with `--nightly`, 650 (`d28ad34`) |
| `fcf343f` | **final code commit**: no dead code in `concord-extras` without `json`/`form`, gate step `clippy-features`, benchmark generator (found by the measurement) | 35/35 with `--nightly`, 650 |

Gate counts are the facade's default workspace lane (`verify-test`); each run also passes every
feature, consumer, package and documentation lane listed by `scripts/verify.sh`.

## Platforms

- Linux x86_64, rustc 1.99.1: every gate step runs locally.
- Windows (and macOS): **NOT_RUN** — native CI execution needs a push, which this increment does
  not authorize. `ci.yml` runs `scripts/verify.sh` on `windows-latest`; the scripts use bash and
  fall back to `python` when `python3` is absent, but that path has not run on Windows.
- Nightly: the non-blocking `Clone` lane (`scripts/verify.sh --nightly`) runs locally.
