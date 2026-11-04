# V4.3 candidate — developer-experience implementation: status

Label: **V4.3 candidate** (not a release; no tag, no publication, no `main` merge).

## Identities

| What | Value |
|---|---|
| Repository | `gqsnt/concord_api` |
| Accepted V4.2 implementation | `ed6703a29d6f798f13e06379d488feeac33410f4` |
| V4.2 release receipt (`concord-v4.2`) | `85c50024f78a75a12570db154bb17a211d2eb673` |
| V4.2 merge, annotated tag `v4.2.0` target | `b2ba34199b4221906c49d2ed5eb60bd85dca3fcd` (tag object `cf6743b210b1132ca9644ceba64c264f3fb73176`) |
| **Base of this work** (`origin/main` at start) | `5ed600b766fa274acbced0b006e70dcd171fc2bd` — records-only successor of `b2ba341` (changes only `v4/release-4.2/*`) |
| Base CI (exact SHA) | run [41851008386](https://github.com/gqsnt/concord_api/actions/runs/41851008386): success |
| Implementation branch | `concord-v4.3-dx-implementation` |
| Generated-input experiment branch | `concord-v4.3-dx-generated-inputs` (see DECISIONS.md) |
| Final implementation SHA | pending |

The campaign's B0 trial sources (`FINAL_EVIDENCE.zip`, the handoff's `reference/campaign/`) are
historical evidence of an older source. Nothing here relabels them as tests of this branch; every
claim below is credited only to commands run on this branch's commits.

## Toolchain and gates

- Toolchain pin and `rust-version`: 1.99.1 (`rust-toolchain.toml`, `Cargo.toml`); local `rustc 1.99.1 (58cf8f9c3 2026-10-13)`, Linux x86_64.
- Maintained gate inventory: `scripts/verify.sh` (CI calls it on Linux and native Windows; nightly lane non-blocking).
- Baseline (`5ed600b`, local, before any change): `scripts/verify.sh --package --bench` — 37/37 PASS in 4m35s.
- Final commands: `scripts/verify.sh --package --bench` and `scripts/verify.sh --nightly-only` on the final commit, plus the branch's CI run.

## Work packages

| WP | Scope | Status | Where |
|---|---|---|---|
| W0 | base, branch, ledger, baseline | done | this file |
| W1 | streaming guidance and example; domain adapters; named workflow; bounded traversal | done, maintained | `examples/native/src/upload.rs`, `examples/app`, guides; DECISIONS S1, D1/W1 |
| W2 | metadata-preserving native-edit recipe; public convenience decision | done, maintained recipe; convenience rejected | `examples/native/src/metadata.rs`; DECISIONS M1 |
| W3 | SDK context boundary; independent outside-endpoint crate; core decision | done, maintained; core primitive rejected | `examples/sdk/src/context.rs`, `examples/extension`; DECISIONS X1 |
| W4 | generated-input ADR; macro prototype; matched evaluation; adoption decision | done: evaluated, not adopted (G1 outcome); prototype retained | branch `concord-v4.3-dx-generated-inputs`; `experiments/generated-inputs/` |
| W5 | integration, review, verification, delivery | in progress | REVIEW_RESOLUTIONS.md, VERIFICATION.md |

Helper branches merged into the implementation branch: `dx43/native` (`e73838e`, `d226b3f`),
`dx43/app` (`4c01954`), `dx43/ext` (`cf3c9fc` = frozen SDK context, `b077e7e`).

## Shared example infrastructure

- `examples/fixture` — wire-recording loopback server (content-length and chunked request
  bodies) and deterministic fake Catalog/Ledger services; a dev-dependency only.
- `examples/sdk` — the Catalog and Ledger declarations the W1/W3/W4 examples share.

## Remaining gates

Pending until the final commit exists.
