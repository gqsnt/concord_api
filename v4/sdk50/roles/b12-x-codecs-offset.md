# Between-wave capsule X: public media check (codecs) and offset-envelope helper (pagination), adopted by FRED and USGS

You work in the Concord Rust workspace (your working directory, a git worktree on branch
`sdk50/b12-x`). Read `AGENTS.md`/`CLAUDE.md` if present. You cannot commit. Network is enabled for
loopback tests; contact no external service. Migration task: you are given the solution shape;
this is not first-use evidence.

## D — strict media/charset check, public in `concord-codecs`

`crates/concord-codecs/src/media.rs` holds the strict `Content-Type` parsing the standard formats
use, but it is private. The FRED SDK (`examples/fred-sdk/src/codecs.rs`: its XML, ZIP and problem
formats) re-implemented singleton `Content-Type` and charset checks with `mime` (its FRICTION.md
F3). Expose the **smallest** public API that lets a custom format author do the same check a
standard format does — e.g. a function taking the `ResponseHead`, the acceptable media types
(type/subtype and `+suffix` rules as the standard formats use them) and a charset policy, returning
`Result<(), CodecError>` with the same static labels. Keep the parser private; one implementation;
no new dependency. Document it in `docs/CUSTOM_FORMATS.md`. Migrate FRED's checks to it, keeping
every FRED assertion unchanged.

## E — offset-envelope helper in `concord-pagination`

FRED (`examples/fred-sdk/src/pagination.rs`, FRICTION F6) and USGS
(`examples/usgs-earthquake-catalog-sdk/src`, offset traversal) hand-write the same checks around
`Pages`: given the request's offset and limit and the page's reported count/length, compute the
next offset with checked arithmetic, detect "did not advance", "more items than the limit",
"offset beyond count" and the clean end. Add a small, provider-neutral helper (a function or a tiny
type) in `concord-pagination` — no knowledge of any provider field names — and migrate both SDKs'
arithmetic to it, keeping their provider projection and every assertion unchanged. Include unit
tests for overflow, zero limit, empty page with remaining count, exact end.

## Also: declared `limit`

Concord now has a `limit N;` declaration statement (AUTHOR_GUIDE §5.10a): it bounds every finite
typed output on all paths, including `prepare()?.send()`. In FRED and USGS, where a send policy
calls `decode_limited(BODY_LIMIT)` (or similar) only to bound a finite success body, declare the
limit and call `decode()`/`decode_with_meta()` instead; keep error-document limits explicit
(`document(…, ERROR_LIMIT)`). Every existing assertion must stay unchanged.

Rules: never edit an existing test's assertions or expected values; add focused tests. Update
`scripts/verify.sh` only if a new feature needs its own step. Bounded verification
(`CARGO_BUILD_JOBS=4`, one cargo command at a time): tests of `concord-codecs` (relevant
features), `concord-pagination` (default and `--features link`), FRED, USGS; clippy and rustdoc
`-D warnings` on what you changed. Report the APIs added (with justification), lines removed/added
per SDK, and test counts.
