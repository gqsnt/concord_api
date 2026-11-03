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

## Platforms

- Linux x86_64, rustc 1.99.1: every gate step runs locally.
- Windows (and macOS): **NOT_RUN** — native CI execution needs a push, which this increment does
  not authorize. `ci.yml` runs `scripts/verify.sh` on `windows-latest`; the scripts use bash and
  fall back to `python` when `python3` is absent, but that path has not run on Windows.
- Nightly: the non-blocking `Clone` lane (`scripts/verify.sh --nightly`) runs locally.
