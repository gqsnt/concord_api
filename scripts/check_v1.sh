#!/usr/bin/env bash
set -euo pipefail

cargo fmt --check
cargo test -p concord_core redaction
cargo test -p concord_core auth_runtime
cargo test -p concord_examples live_smoke
cargo test -p concord_core
cargo test -p concord_macros
cargo test -p concord_examples
cargo test --workspace
cargo doc --workspace --no-deps
cargo clippy --workspace --all-targets -- -D warnings
