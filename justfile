# List contributor commands.
[private]
default:
    @just --list

# Run property tests; set PROPTEST_CASES to change the default 256 cases.
[no-exit-message]
props:
    @python3 -B tools/gate.py props

# Check Rust formatting.
[no-exit-message]
fmt:
    @python3 -B tools/gate.py fmt

# Check every workspace target with default and optional features.
[no-exit-message]
check:
    @python3 -B tools/gate.py check

# Run Clippy with warnings treated as errors.
[no-exit-message]
lint:
    @python3 -B tools/gate.py lint

# Run tests and doctests; CONCORD_SKIP_SOCKETS=1 reports and skips local-socket cases.
[no-exit-message]
test:
    @python3 -B tools/gate.py test

# Build documentation with warnings treated as errors.
[no-exit-message]
doc:
    @python3 -B tools/gate.py doc

# Check the workspace on Rust 1.96.
[no-exit-message]
msrv:
    @python3 -B tools/gate.py msrv

# Check Concord feature combinations with forwarded features off and on, and each forwarder.
[no-exit-message]
features:
    @python3 -B tools/gate.py features

# Count unfinished todo! calls in workspace crates.
[no-exit-message]
todo:
    @python3 -B tools/gate.py todo

# List tests by contract; --strict rejects gaps without a reason in tools/NO-TEST.md.
[no-exit-message]
matrix *args:
    @python3 -B tools/matrix.py {{args}}

# Check files and heading anchors in every tracked Markdown page.
[no-exit-message]
links:
    @python3 -B tools/links.py

# Run all workspace checks and print each result; supports CONCORD_SKIP_SOCKETS=1.
[no-exit-message]
gate:
    @python3 -B tools/gate.py gate

# Verify tests catch each source mutation; supports CONCORD_SKIP_SOCKETS=1.
[no-exit-message]
mutants *args:
    @python3 -B tools/mutate.py {{args}}

# Run opt-in timing and allocation measurements; --expansion [FILE] measures the macro.
[no-exit-message]
bench *args:
    @python3 -B tools/bench.py {{args}}

# Check panic-prone operations have local safety reasons.
[no-exit-message]
panics:
    @python3 -B tools/panics.py --list --compiler

# Check feature combinations and detached crates on stable and Rust 1.96.
[no-exit-message]
features-all:
    @python3 -B tools/features.py

# Check public documentation and the rendered facade API.
[no-exit-message]
docs-audit:
    @python3 -B tools/docs.py

# Check whole documentation descriptions against their pages and compile them.
[no-exit-message]
doc-examples:
    @cargo test --offline -p real-apis --test pages

# Package all five libraries and check their unpacked contents and feature sets.
[no-exit-message]
package:
    @python3 -B tools/package.py

# Run gate, documentation checks, safety, packaging, feature isolation, and mutations.
[no-exit-message]
release:
    @python3 -B tools/release.py
