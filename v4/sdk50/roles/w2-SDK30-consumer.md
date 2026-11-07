# Fresh consumer capsule — SDK30 HubSpot CRM

You are a **fresh consumer** of the SDK in `examples/hubspot-crm-sdk` (package `concord-sdk-hubspot-crm`), which you did not write.
Read only its public documentation first: `examples/hubspot-crm-sdk/README.md` and the rustdoc you generate
(`cargo doc -p concord-sdk-hubspot-crm --no-deps`), plus Concord's public docs if needed. Do **not** read
`examples/hubspot-crm-sdk/src`, its `tests/`, `FRICTION.md` or `PROVENANCE.md` before your first working call. If
you must, write down what and why first — it changes the evidence label, not your permission.

## Provider slice: SDK30 · HubSpot CRM (complex; messaging and CRM)

Connected workflows (both required):
1. Search contacts, follow paging.next.after, then retrieve a chosen contact.
2. Create a contact and later look it up by ID.

Contract notes: OAuth access/refresh token with possible replacement refresh token; search continuation; write ambiguity.
Failure that must be exercised: Concurrent refresh must not overwrite a newer replacement token; a timed-out create remains uncertain.
Excluded: Marketing automation, all object types and exactly-once contact creation.
Official documentation (read-only research is allowed; cite the exact pages and the date you read
them):
- https://developers.hubspot.com/blog/production-ready-oauth-token-management-hubspot-integrations
- https://developers.hubspot.com/blog/a-developers-guide-to-hubspot-crm-objects-contacts-object
Live provider calls: **NOT_RUN** — never call the real service; every workflow runs against local
fixtures (`examples/testserver`: `TestServer::start(|request: &Recorded| -> Reply)`).

## Fixed rules

- Concord's product is its core, macros, declarations and generated usage; the hierarchy
  client → scope → endpoint is fixed. The frozen baseline is commit `a5e03c44173ca3f4ee44248fb6152938c3d94f42`: **do not edit
  any shared crate** (`crates/*`, `checks/*`, other examples, `scripts/*`) — record what you would
  change instead. Your ownership is `examples/hubspot-crm-sdk/` plus adding `"examples/hubspot-crm-sdk"` to the root `Cargo.toml`
  workspace `members` (and the `Cargo.lock` entry cargo writes).
- Build into the worktree's default `target/` (never set a relative `CARGO_TARGET_DIR`);
  `CARGO_BUILD_JOBS=4`, one cargo command at a time.
- Adding your package changes `Cargo.lock`: build once without `--locked` (offline if possible),
  then use `--locked`. Prefer dependencies already in `Cargo.lock`; justify any new crate (name,
  version, license, why) in `PROVENANCE.md`. No other package may change version in the lock.
- You cannot commit (`.git` is read-only); leave your work in the working tree.
- No private reasoning transcripts are wanted: record observable actions, diagnostics, files read
  and decisions.

## Before writing code: predict

Write `examples/hubspot-crm-sdk/FIRST_USE.md` with a section "Predictions" answering: for each workflow, which calls
you will make, what they send on the wire (method, path, query, headers), what they return, and
when/how they fail (status errors, provider errors, malformed bodies, pagination end, secrets).

## Then build a small application (in `examples/hubspot-crm-sdk/tests/first_use.rs`)

Two caller tasks, one per workflow, written as tests against the loopback server with realistic
values and assertions on wire requests, errors, metadata, completion and partial results. Use the
ordinary caller path; use advanced APIs only where the task needs them.

Append to `FIRST_USE.md`: your first working call (what it took), every friction incident in
order (unneeded imports or types, unclear completion, surprising generated names, misleading
diagnostics, source lookups), a comparison of each prediction with observed behaviour, and the
assistance label (`docs-only` or `source-assisted: <files>`). Do not edit the SDK's `src/`.

## Report (final message)
Test results, prediction mismatches, friction list, assistance label.
