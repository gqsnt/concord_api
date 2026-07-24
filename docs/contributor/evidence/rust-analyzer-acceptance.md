# Rust-analyzer acceptance

This evidence belongs to Closure C7 and records command-line analysis
separately from interactive editor behavior.

## Tool and workspace

- Tool: `rust-analyzer 1.97.1 (8bab26f4 2026-07-14)`.
- Host: `x86_64-pc-windows-msvc`.
- Root workspace: `F:\projects\concord_api`.
- Fixture isolation: the root Cargo workspace excludes `fixtures`; the
  repository policy check passes that fact.

## Automated supplementary results

| Command | Observed result |
|---|---|
| `rust-analyzer --version` | Exit 0; version 1.97.1. |
| `rust-analyzer -q analysis-stats . --run-all-ide-things` | Exit 0; 50 crates, 339 workspace item trees, 74,999 workspace lines, 250 IDE files, zero `fixtures/msrv` path matches. |
| `rust-analyzer -q diagnostics .` | Exit 1; 118 `unresolved-macro-call` diagnostics, all naming `emit_generated_raw_response_method`, with E0282=209, E0432=52, E0277=28, and E0599=5 cascades. |
| In-memory path/error count over the diagnostics output | Zero `fixtures/msrv` paths, zero pagination `offset`/`page_size` associated-field errors, and zero authentication `HeaderName` move errors. |
| `cargo run --locked -p concord-xtask -- fixtures` | Exit 0; isolated fixture metadata and all 7 serialized public-compile cases passed. |

The remaining analyzer diagnostics originate where proc-macro output invokes
the façade's feature-selected `macro_rules!` raw-response bridge. Rustc,
warning-denied Clippy, stable exact UI with and without all features, and both
workspace suites compile that surface successfully. Directly changing the
generated path did not remove the analyzer limitation and was reverted so the
accepted façade-only generated architecture remains intact.

The automated command-line result is therefore not reported as clean.
Interactive behavior may differ because an editor supplies active feature,
target, and proc-macro configuration; it must be observed directly.

## Manual acceptance protocol

No graphical editor was available in the agent environment. No row below is
inferred from compiler tests or command-line analysis.

| # | Acceptance row | Status | Evidence required to close |
|---:|---|---|---|
| 1 | Completion after a direct endpoint call | Pending external editor session | Open a generated client call and record the offered request methods/terminals. |
| 2 | Hover shows the named request wrapper | Pending external editor session | Hover the direct call result and record the named wrapper type. |
| 3 | Optional setters and terminals appear without imports | Pending external editor session | Confirm inherent setters/terminals without a generated trait or prelude import. |
| 4 | A non-paginated endpoint does not offer `paginate` | Pending external editor session | Record completion for a non-paginated wrapper. |
| 5 | Go-to-definition reaches the declaration or a stable generated item | Pending external editor session | Navigate from an endpoint and record the destination. |
| 6 | A renamed façade dependency works | Pending external editor session | Open the renamed-façade fixture independently and record completion/navigation. |
| 7 | Feature-disabled methods are absent | Pending external editor session | Disable the relevant features and record the generated surface. |
| 8 | Diagnostics point to DSL spans | Pending external editor session | Introduce or open a maintained invalid declaration and record the editor span. |
| 9 | Fixture workspaces do not pollute root analysis | Pending external editor session | Open only the root workspace and confirm fixture packages are absent from workspace symbols/diagnostics. |

Owner and next action:
A repository maintainer with a graphical Rust editor must execute all nine
rows using the pinned project toolchain and record the editor, extension
version, feature configuration, and observed result. The maintainer should
also recheck the remaining nested façade macro diagnostics with the newest
approved rust-analyzer and report them upstream if independently reproduced.
