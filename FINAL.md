# Concord v1 freeze

Concord v1.0.0 is the final frozen state of this private project.

The DSL, generated client surface, runtime contracts, and documentation are
frozen at the annotated `v1.0.0` tag.

Changes may fix implementation defects or documentation errors without adding
public syntax, generated capabilities, or runtime escape paths.

## Non-goals

- Publication, crates.io metadata, and release packaging are excluded.
- OpenAPI import is excluded.
- Request coalescing is excluded.
- A website is excluded.

Finalization judgments are recorded in
[`dev_doc/finalization_decisions.md`](dev_doc/finalization_decisions.md).
