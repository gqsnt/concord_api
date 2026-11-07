# Independent maintenance capsule — SDK37 Strava

You maintain the SDK in `examples/strava-sdk` (package `concord-sdk-strava`), which you did not write, after its first
consumer. First attempt the ticket below **before** reading `FRICTION.md` or `FIRST_USE.md`.

## Provider slice: SDK37 · Strava (medium; social and media)

Connected workflows (both required):
1. Page an authorized athlete’s activities, then retrieve one activity.
2. Upload an activity file and poll the returned upload ID for processing status.

Contract notes: OAuth authorization-code refresh; separate short and daily usage windows; multipart uploads are asynchronously processed.
Failure that must be exercised: An upload timeout is ambiguous and a one-shot file is not replayed automatically; 429 applies to more than one window.
Excluded: Routes, segment competitions, webhook hosting and automatic re-upload.
Official documentation (read-only research is allowed; cite the exact pages and the date you read
them):
- https://developers.strava.com/docs/authentication/
- https://developers.strava.com/docs/rate-limits/
- https://developers.strava.com/docs/uploads/
Live provider calls: **NOT_RUN** — never call the real service; every workflow runs against local
fixtures (`examples/testserver`: `TestServer::start(|request: &Recorded| -> Reply)`).

## Fixed rules

- Concord's product is its core, macros, declarations and generated usage; the hierarchy
  client → scope → endpoint is fixed. The frozen baseline is commit `c3fd4f1ee30c1727e26b285270f2cc8d8b694e77`: **do not edit
  any shared crate** (`crates/*`, `checks/*`, other examples, `scripts/*`) — record what you would
  change instead. Your ownership is `examples/strava-sdk/` plus adding `"examples/strava-sdk"` to the root `Cargo.toml`
  workspace `members` (and the `Cargo.lock` entry cargo writes).
- Build into the worktree's default `target/` (never set a relative `CARGO_TARGET_DIR`);
  `CARGO_BUILD_JOBS=4`, one cargo command at a time.
- Adding your package changes `Cargo.lock`: build once without `--locked` (offline if possible),
  then use `--locked`. Prefer dependencies already in `Cargo.lock`; justify any new crate (name,
  version, license, why) in `PROVENANCE.md`. No other package may change version in the lock.
- You cannot commit (`.git` is read-only); leave your work in the working tree.
- No private reasoning transcripts are wanted: record observable actions, diagnostics, files read
  and decisions.
- Read nothing outside your worktree except the official documentation pages above and the
  Rust toolchain / cargo registry sources: in particular, not the campaign files in its parent
  directories (mission, contracts, records, other roles' briefs or worktrees).

## Ticket

Callers need to update an activity they own: its name, sport type, description, gear and the
commute/trainer flags, with `PUT /activities/{id}` (an `activity:write` scope is required); the
response is the updated activity. Read the activities reference (updateActivityById) and the
authentication page (scopes) and cite them with the date:
- https://developers.strava.com/docs/reference/
- https://developers.strava.com/docs/authentication/

Add the update so a caller sets only the fields it means to change and gets the updated activity
back; a token without `activity:write` is a typed authorization error; an update that times out is
uncertain (the caller re-reads the activity) and is never resent automatically; the update counts
against the same usage windows as other requests. Keep listing, retrieval and uploads unchanged
for existing callers.


## Rules

Make a real change to the SDK implementation or its caller contract, grounded in the provider's
documentation (cite it). Keep the existing consumer (`tests/first_use.rs`) and every existing
assertion; never weaken one. Add a focused regression test and, if the change adds behavior, a
consumer test for it. Then write `examples/strava-sdk/MAINTENANCE.md`: what changed, every file/location you
had to edit and which facts had to be changed in several places, whether the compiler or the tests
caught an omitted edit, remaining generic duplication, provider-specific responsibilities, and
candidate Concord improvements. State prior exposure (what you read before starting).

## Report (final message)
Diff summary, locations touched, test results, findings.
