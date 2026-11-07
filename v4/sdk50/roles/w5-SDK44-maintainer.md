# Independent maintenance capsule — SDK44 Backblaze B2 Native

You maintain the SDK in `examples/backblaze-b2-native-sdk` (package `concord-sdk-backblaze-b2-native`), which you did not write, after its first
consumer. First attempt the ticket below **before** reading `FRICTION.md` or `FIRST_USE.md`.

## Provider slice: SDK44 · Backblaze B2 Native (complex; infrastructure and observability)

Connected workflows (both required):
1. Authorize account and obtain an upload URL, then upload a small file with required length and SHA-1.
2. Download that file by ID and verify its returned checksum.

Contract notes: Native application-key auth returns API/download URLs; upload URL has a distinct token; 503 can require a new upload URL.
Failure that must be exercised: A new upload URL alone cannot replay a consumed stream or prove an earlier ambiguous upload did not succeed.
Excluded: Large-file parts, lifecycle rules, S3 compatibility, server-side encryption variants and silent exactly-once guarantees.
Official documentation (read-only research is allowed; cite the exact pages and the date you read
them):
- https://www.backblaze.com/apidocs/b2-authorize-account
- https://www.backblaze.com/apidocs/b2-get-upload-url
- https://www.backblaze.com/apidocs/b2-upload-file
- https://www.backblaze.com/apidocs/b2-download-file-by-id
Live provider calls: **NOT_RUN** — never call the real service; every workflow runs against local
fixtures (`examples/testserver`: `TestServer::start(|request: &Recorded| -> Reply)`).

## Fixed rules

- Concord's product is its core, macros, declarations and generated usage; the hierarchy
  client → scope → endpoint is fixed. The frozen baseline is commit `c3fd4f1ee30c1727e26b285270f2cc8d8b694e77`: **do not edit
  any shared crate** (`crates/*`, `checks/*`, other examples, `scripts/*`) — record what you would
  change instead. Your ownership is `examples/backblaze-b2-native-sdk/` plus adding `"examples/backblaze-b2-native-sdk"` to the root `Cargo.toml`
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

Callers need to list a bucket's file names and delete a file version: `b2_list_file_names`
(paginated by `startFileName` / `nextFileName`, with `prefix`, `delimiter` and `maxFileCount`)
and `b2_delete_file_version` (by file name and file id). Read both references and cite them with
the date:
- https://www.backblaze.com/apidocs/b2-list-file-names
- https://www.backblaze.com/apidocs/b2-delete-file-version

Add a file-name traversal on the storage scope (the same prefix and delimiter on every page, a
page cap and a resume point, completion when `nextFileName` is null) and a version delete (a
timed-out delete is uncertain: the caller lists again; never resent automatically; a file id and
name that do not match is a typed provider error). Both use the account authorization and the
API URL issued for it. Keep authorization, upload and download unchanged for existing callers.


## Rules

Make a real change to the SDK implementation or its caller contract, grounded in the provider's
documentation (cite it). Keep the existing consumer (`tests/first_use.rs`) and every existing
assertion; never weaken one. Add a focused regression test and, if the change adds behavior, a
consumer test for it. Then write `examples/backblaze-b2-native-sdk/MAINTENANCE.md`: what changed, every file/location you
had to edit and which facts had to be changed in several places, whether the compiler or the tests
caught an omitted edit, remaining generic duplication, provider-specific responsibilities, and
candidate Concord improvements. State prior exposure (what you read before starting).

## Report (final message)
Diff summary, locations touched, test results, findings.
