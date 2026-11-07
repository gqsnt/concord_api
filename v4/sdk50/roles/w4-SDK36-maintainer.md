# Independent maintenance capsule — SDK36 Spotify Web API

You maintain the SDK in `examples/spotify-web-api-sdk` (package `concord-sdk-spotify-web-api`), which you did not write, after its first
consumer. First attempt the ticket below **before** reading `FRICTION.md` or `FIRST_USE.md`.

## Provider slice: SDK36 · Spotify Web API (medium; social and media)

Connected workflows (both required):
1. Search for artists and retrieve a selected artist.
2. List an authorized user’s playlists and fetch a selected playlist.

Contract notes: OAuth authorization-code and refresh; pagination and 429; development-mode resource access may differ.
Failure that must be exercised: A missing replacement refresh token must not erase the existing token; 403 may indicate quota mode.
Excluded: Playback control, audio download, recommendations and assuming every account has extended access.
Official documentation (read-only research is allowed; cite the exact pages and the date you read
them):
- https://developer.spotify.com/documentation/web-api/concepts/api-calls
- https://developer.spotify.com/documentation/web-api/tutorials/refreshing-tokens
- https://developer.spotify.com/documentation/web-api/concepts/quota-modes
Live provider calls: **NOT_RUN** — never call the real service; every workflow runs against local
fixtures (`examples/testserver`: `TestServer::start(|request: &Recorded| -> Reply)`).

## Fixed rules

- Concord's product is its core, macros, declarations and generated usage; the hierarchy
  client → scope → endpoint is fixed. The frozen baseline is commit `c3fd4f1ee30c1727e26b285270f2cc8d8b694e77`: **do not edit
  any shared crate** (`crates/*`, `checks/*`, other examples, `scripts/*`) — record what you would
  change instead. Your ownership is `examples/spotify-web-api-sdk/` plus adding `"examples/spotify-web-api-sdk"` to the root `Cargo.toml`
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

Callers need to add tracks to a playlist they own: `POST /v1/playlists/{playlist_id}/tracks`
with up to 100 item URIs and an optional insert `position`; Spotify answers `201` with the
playlist's new `snapshot_id`. Read the add-items reference and cite it with the date:
- https://developer.spotify.com/documentation/web-api/reference/add-tracks-to-playlist

Add the call so a caller gets the new snapshot id; more than 100 URIs or a malformed URI is
refused before any request; the write uses the existing session and token rules; a timed-out add
is uncertain (the items may have been added: the caller re-reads the playlist) and is never
resent automatically, including by the 401 refresh-and-replay path. Keep search, artist and
playlist reads unchanged for existing callers.


## Rules

Make a real change to the SDK implementation or its caller contract, grounded in the provider's
documentation (cite it). Keep the existing consumer (`tests/first_use.rs`) and every existing
assertion; never weaken one. Add a focused regression test and, if the change adds behavior, a
consumer test for it. Then write `examples/spotify-web-api-sdk/MAINTENANCE.md`: what changed, every file/location you
had to edit and which facts had to be changed in several places, whether the compiler or the tests
caught an omitted edit, remaining generic duplication, provider-specific responsibilities, and
candidate Concord improvements. State prior exposure (what you read before starting).

## Report (final message)
Diff summary, locations touched, test results, findings.
