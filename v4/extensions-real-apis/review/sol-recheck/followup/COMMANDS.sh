#!/usr/bin/env bash
# Run from the repository root. Generated outputs stay under followup/.
set -euo pipefail
export CARGO_TARGET_DIR="$PWD/review/sol-recheck/followup/target"
export TMPDIR="$PWD/review/sol-recheck/followup/tmp"
mkdir -p "$TMPDIR"

cargo test --offline \
  -p concord-example-github-sdk -p concord-example-github-app \
  -p concord-check-maintenance-github \
  -p concord-example-kubernetes-sdk -p concord-example-kubernetes-app \
  -p concord-check-maintenance-kubernetes -p concord-example-shopify-app

cargo test --offline --locked \
  --manifest-path review/sol-recheck/followup/probes/Cargo.toml \
  --tests -- --nocapture
