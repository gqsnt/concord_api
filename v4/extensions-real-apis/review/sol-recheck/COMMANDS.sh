#!/usr/bin/env bash
# Run from the repository root. All generated artifacts stay in the scratch tree.
set -euo pipefail
export CARGO_TARGET_DIR="$PWD/review/sol-recheck/target"
export TMPDIR="$PWD/review/sol-recheck/tmp"
mkdir -p "$TMPDIR"

cargo test --offline \
  -p concord-example-github-sdk -p concord-example-github-app \
  -p concord-example-kubernetes-sdk -p concord-example-kubernetes-app \
  -p concord-example-salesforce-sdk -p concord-example-salesforce-app \
  -p concord-example-shopify-app -p concord-check-stream-stress \
  -p concord-pagination -p concord-check-maintenance-github \
  -p concord-check-maintenance-kubernetes -p concord-check-maintenance-salesforce \
  -p concord-check-maintenance-shopify
cargo test --offline -p concord-extras --all-features
cargo test --offline -p concord-example-stripe-sdk -p concord-example-shopify-sdk

python3 review/sol-recheck/setup_baseline.py
cargo test --offline --locked --manifest-path review/sol-recheck/probes/Cargo.toml --tests -- --nocapture
cargo test --offline -p concord-extras --all-features --test csv_write_memory -- --nocapture
python3 review/sol-recheck/run_memory.py
python3 review/sol-recheck/repeat_jsonl.py
