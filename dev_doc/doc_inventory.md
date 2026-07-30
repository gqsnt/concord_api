# Pre-finalization documentation inventory

This inventory records the documentation graph before the Phase 2 consolidation.

## Scope

- `README.md` is the repository entry point.
- `docs/SUMMARY.md` lists all 75 Markdown chapter targets under `docs/`.
- `docs/README.md` is the documentation landing page.
- `CHANGELOG.md` records repository history.
- `fixtures/README.md` documents locked dependency-graph fixtures.
- `tools/perf/README.md` documents internal performance probes.
- `dev_doc/finalization_decisions.md` begins the finalization record.

## Documentation files

`docs/SUMMARY.md` is the canonical file-by-file inventory for the pre-finalization `docs/` tree. Its entries cover the landing page and every page in `start/`, `concepts/`, `guide/`, `features/`, `recipes/`, `extending/`, `reference/`, and `contributor/`.

## References

The repository documentation checker parses every Markdown link in `README.md` and `docs/`, resolves every local file and anchor, rejects links that escape `docs/` from a documentation page, validates every mdBook include target and line selector, and requires the `docs/SUMMARY.md` inventory to match the filesystem.

The Phase 0 run reported:

- 75 unique chapter targets in `docs/SUMMARY.md`;
- 602 local file references;
- 41 local anchor references;
- 87 source include directives;
- 2 external URLs syntax-counted without network access;
- no missing files, anchors, or include selectors.

Run `cargo run -p concord-xtask -- docs` to reproduce the reference inventory and resolution check for this revision.
