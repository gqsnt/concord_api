# Concord controller files

This directory contains the stable controller prompt and a compact operational project-state index.
It does not replace the normative specifications, implementation program, phase reports, tests, or
conformance evidence.

## Files

- `CONCORD_CONTROLLER_PROMPT.md` is the prompt used in a fresh online ChatGPT controller
  conversation.
- `PROJECT_CONTROL.toml` points to the current authorities and records the latest independently
  validated source/evidence/anchor chain and the active planned phase.

`PROJECT_CONTROL.toml` is deliberately small. It records pointers and expected relationships, not
copied requirements or implementation detail. Controllers must verify it before using it as current
fact.

## Commit identity rule

A committed file cannot contain its own Git object identity. The manifest therefore records the
latest validated phase anchor, not an expected current `HEAD`. Later governance-only commits are
valid when the recorded anchor remains an ancestor and their changes are compatible.

After a phase is independently validated, the implementation agent updates the manifest in the
phase evidence/documentation commit with:

- the completed phase and verdict;
- the immutable tested source commit;
- the complete evidence/report commit;
- the phase anchor according to the repository's established convention;
- the next phase identifier, title, status, and planning authority.

The independent controller then verifies those values against Git and the repository before
accepting the phase.

## Session model

Use a fresh controller conversation for each phase lifecycle:

```text
bootstrap → mission → implementation → review → correction if needed → validation → handoff
```

The next controller conversation starts with bootstrap again. Conversation history is not durable
project state.
