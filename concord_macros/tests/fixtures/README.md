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
- `json_disabled` omits the runtime `json` feature and proves that generated
  JSON syntax produces the focused feature diagnostic rather than falling
  back to another body or response mode.

Ordinary generated construction and execution, builder behavior, lazy request
creation, and nested/repeated expansion live in `generated_surface.rs`.
Source-only failures live under `tests/ui/fail` and run through Trybuild.
