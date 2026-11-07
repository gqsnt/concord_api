# Independent maintenance capsule — SDK41 DigitalOcean

You maintain the SDK in `examples/digitalocean-sdk` (package `concord-sdk-digitalocean`), which you did not write, after its first
consumer. First attempt the ticket below **before** reading `FRICTION.md` or `FIRST_USE.md`.

## Provider slice: SDK41 · DigitalOcean (small; infrastructure and observability)

Connected workflows (both required):
1. List droplets with links.pages, then retrieve a droplet.
2. List the selected droplet’s actions, then retrieve one action.

Contract notes: Bearer token; body next links, with optional links when no next page exists.
Failure that must be exercised: A missing next link means completion; never forward bearer credentials to an untrusted host.
Excluded: Creating or deleting droplets, billing and broad cloud coverage.
Official documentation (read-only research is allowed; cite the exact pages and the date you read
them):
- https://docs.digitalocean.com/products/droplets/reference/api/droplets/
- https://docs.digitalocean.com/reference/api/reference/public-apis/
Live provider calls: **NOT_RUN** — never call the real service; every workflow runs against local
fixtures (`examples/testserver`: `TestServer::start(|request: &Recorded| -> Reply)`).

## Fixed rules

- Concord's product is its core, macros, declarations and generated usage; the hierarchy
  client → scope → endpoint is fixed. The frozen baseline is commit `c3fd4f1ee30c1727e26b285270f2cc8d8b694e77`: **do not edit
  any shared crate** (`crates/*`, `checks/*`, other examples, `scripts/*`) — record what you would
  change instead. Your ownership is `examples/digitalocean-sdk/` plus adding `"examples/digitalocean-sdk"` to the root `Cargo.toml`
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

Callers need two more read paths: listing only the droplets that carry a tag
(`GET /v2/droplets?tag_name=…`, which the droplets reference documents alongside the plain
listing) and listing a droplet's snapshots (`GET /v2/droplets/{id}/snapshots`, paginated with
the same `links.pages` object). Read the droplets reference and cite it with the date:
- https://docs.digitalocean.com/products/droplets/reference/api/droplets/

Add both: a tag-filtered droplet traversal (the tag is validated before any request and kept on
every page of the traversal, including a resumed one) and a snapshot traversal for one droplet,
each with the same completion, cap, resume and untrusted-link rules as the existing listings.
Keep the existing droplet and action listings and retrievals unchanged for existing callers.


## Rules

Make a real change to the SDK implementation or its caller contract, grounded in the provider's
documentation (cite it). Keep the existing consumer (`tests/first_use.rs`) and every existing
assertion; never weaken one. Add a focused regression test and, if the change adds behavior, a
consumer test for it. Then write `examples/digitalocean-sdk/MAINTENANCE.md`: what changed, every file/location you
had to edit and which facts had to be changed in several places, whether the compiler or the tests
caught an omitted edit, remaining generic duplication, provider-specific responsibilities, and
candidate Concord improvements. State prior exposure (what you read before starting).

## Report (final message)
Diff summary, locations touched, test results, findings.
