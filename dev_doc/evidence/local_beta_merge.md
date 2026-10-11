# Local Concord v2.1 beta integration

**Status:** Historical v2.1 local-merge evidence from the v2.1 repository,
superseded by the v3 lifecycle in
[`../release_gate_status.md`](../release_gate_status.md). The `main` branch and
SHAs below belong to that repository's history, not to this v3 tree.

- Pre-merge `main`: `f7464c58f4a60b7332547e8f91039a269171a6ff`
- Verified release parent: `bc4efb03f6d3a2c8b6c39bd203157e6a192d6e4e`
- Merge method: local `--no-ff`, preserving the release history
- Version: `2.1.0-beta.1`
- Publication: disabled in every intended public package

The merge commit containing this record has the pre-merge `main` and verified
release candidate as its two parents. Its exact object ID cannot be embedded
in its own contents. The terminal Windows/Linux `just verify`, explicit locked
workspace/docs/release/export commands, parent-topology check, and clean status
are therefore recorded by the operator output and final handoff against the
exact resulting object ID.

Local beta integration is valid only if that terminal matrix exits 0 on both
supported platforms. A failed terminal check invalidates the integration
decision until repaired through normal commits. No macOS run is required by
the selected support boundary.

This local merge is not a stable-release or publication claim. No push, tag,
registry action, license choice, public package-name claim, or human evidence
is authorized or implied.
