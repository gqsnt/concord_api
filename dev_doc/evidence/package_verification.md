# Package and downstream-artifact verification

- **Recorded:** 2026-09-04
- **Version under test:** `2.1.0-rc.1`
- **Maintained command:** `pwsh -NoLogo -NoProfile -File scripts/verify_packaged_downstream.ps1`
- **Generated run root:** `target/package-verification/<random-id>/` (untracked)

The four public manifests (`concord-core`, `concord-macros`, `concord-extras`,
and `concord`) are packageable. Examples, test support, xtask, and performance
tools remain `publish = false`.

The script packages public crates in dependency order with `--locked` and
`--no-verify`. Because the RC dependencies do not exist on crates.io, packaging
uses Cargo `[patch.crates-io]` command configuration pointing at the exact
workspace sources. This affects local resolution while assembling each archive;
Cargo's normalized packaged manifests retain versioned registry dependencies.

| Archive | Included files | Compressed size |
|---|---:|---:|
| `concord-core-2.1.0-rc.1.crate` | 184 | 349.0 KiB |
| `concord-macros-2.1.0-rc.1.crate` | 39 | 121.6 KiB |
| `concord-extras-2.1.0-rc.1.crate` | 24 | 41.8 KiB |
| `concord-2.1.0-rc.1.crate` | 311 | 129.0 KiB |

The script extracts all four archives to a fresh random directory and creates a
downstream binary whose `[patch.crates-io]` entries reference only the extracted
archive contents, never workspace crate paths. That binary enables `json` and
`records`, compiles a contract inheriting `Json`, checks public contract
provenance, and runs it.

On the clean-worktree run at implementation commit `9470e24`, downstream
`cargo check --locked` passed in 14.26 s and `cargo run --locked` passed in
15.14 s. Cargo emitted package-metadata warnings
because the repository does not declare license/homepage/repository fields; it
did not omit files or fail packaging. No license was invented as part of this
technical remediation.

The missing license is nevertheless a release failure: `cargo deny check`
rejects every public Concord package as unlicensed. Archive mechanics passing
does not imply permission to publish or redistribute. Only the project owner
can supply that legal decision; the release workflow must then be rerun.

`cargo publish --dry-run` was not run: the RC's exact inter-crate versions are
deliberately absent from the external registry, so it would test registry state
rather than package contents. No publish, push, tag, or other remote mutation
occurred.
