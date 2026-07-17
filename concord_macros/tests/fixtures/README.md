# Manifest-dependent macro fixtures

These are standalone Cargo projects only because their contract depends on
package metadata or dependency topology:

- `renamed_core` proves that generated code resolves a caller-visible alias
  for the `concord_core` package.
- `missing_core` proves the focused diagnostic when the caller manifest omits
  the required runtime package.
- `contract_mismatch` selects a deliberately incompatible fake
  `concord_core` package and proves the generated contract marker fails during
  compilation. Its nested package therefore also requires a manifest.

Ordinary generated use, nested/repeated expansion, equal names, and minimal
trait bounds live in `generated_surface.rs`. Source-only failures live under
`tests/ui/fail` and run through Trybuild.
