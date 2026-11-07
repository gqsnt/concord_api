# Maintenance record: issue status update

Role: independent maintainer of `concord-sdk-sentry` (not its author or first consumer).
Date: 2026-11-07. Baseline `c3fd4f1e` plus the author and first-consumer commits (`d043d589`,
`ae6e6e17`); nothing committed by this role. Live provider calls: **NOT_RUN**. The model, as
reported by the harness's session information: `claude-opus-5-5`; requested model/effort and
observed effort were not visible to this role.

## Prior exposure (read before and while making the change)

- The role brief; the worktree inventory; this package's `src/*`, `README.md`, `PROVENANCE.md`,
  `Cargo.toml`, `tests/{support/mod,protocol,workflows,first_use}.rs` and four fixtures.
- Concord docs: `docs/CHEATSHEET.md`; `docs/AUTHOR_GUIDE.md` §5 (intro), §5.3, §5.4, §5.11b,
  §5.12–5.13, §9.
- Concord source: `concord-retry/src/concord.rs` (whole), `concord-ratelimit/src/lib.rs`
  (`Limiter::new` capacity, lease/acquire FIFO wait), `concord-core/src/error.rs` (`Delivery`,
  `delivery()`), `concord-core/src/response.rs` (`Operation::method`, by search);
  `examples/testserver/src/lib.rs` (`Recorded`, `Reply`, `Gate`).
- Official Sentry pages, 2026-11-07 (exact list and facts in `PROVENANCE.md`):
  [update-an-issue](https://docs.sentry.io/api/events/update-an-issue/),
  [bulk-mutate-an-organizations-issues](https://docs.sentry.io/api/events/bulk-mutate-an-organizations-issues/)
  (only to look for `statusDetails` sub-fields), [ratelimits](https://docs.sentry.io/api/ratelimits/),
  [pagination](https://docs.sentry.io/api/pagination/), [events](https://docs.sentry.io/api/events/).
- `FRICTION.md` and `FIRST_USE.md` were read **only after** the change and its tests passed.
  They changed nothing in the code. No other SDK example and no campaign file was opened.

## What changed for callers

`organization.issue(id).update_status(change).send()` sends
`PUT /api/0/organizations/{org}/issues/{issue}/` and returns `Response<Issue>`, the updated issue.

- `StatusUpdate` (`#[non_exhaustive]`) is the closed status set: `Unresolved`, `Resolved`,
  `ResolvedInNextRelease`, `Ignored(Ignore)`. A status outside the set cannot be written; the
  refusal happens at compile time.
- `Ignore` has private state and checked constructors: `FOREVER`, `for_minutes`,
  `until_seen(times, window)` and `until_users(users, window)`. A zero value, or a window over
  `MAX_IGNORE_WINDOW_MINUTES` (10 080 = 7 days), is `Error::InvalidUpdate(reason)` before any
  request exists.
- The body is `status` plus only that condition's `ignore*` fields. No other attribute and no
  `null` is sent.
- The write takes its own fifth endpoint bucket, under the same `Settings`. It has the same quota
  header observation and the same per-attempt lease-then-frequency admission as the reads.
- Only a 429 is retried, after its Retry-After and both admissions again. A dispatched update
  with no response head becomes the new `Error::Uncertain { transport, report }` and is never
  resent; the caller re-reads the issue.
- A 5xx stays `Error::Provider` and is not retried; the docs say to re-read.
- A deadline during 429 backoff stays `RetryStopped`.
- Reads are unchanged: same keys, same classification. All 9 + 14 + 15 earlier tests pass with no
  edits.

**Semver note.** `Error` is not `#[non_exhaustive]`, so the two new variants break callers that
match on it exhaustively. That is acceptable for this unpublished 0.1.0 package, and the existing
consumer is not affected. Making `Error` non-exhaustive is a separate decision for the owner.

## Decisions and their grounds

| Decision | Ground | Alternative not taken |
| --- | --- | --- |
| `ignore*` fields at the top level of the body | The update page documents them as body parameters | Nested under `statusDetails`: the nested form is seen only in the bulk page's *response* example |
| No `inRelease`/`inCommit` | The update page lists no `statusDetails` sub-fields | Adopting them from a response example |
| `ResolvedInNextRelease` uses the status value `resolvedInNextRelease` | That value is in the update page's `status` enum | `statusDetails.inNextRelease` |
| No `muted`, `substatus` or `snoozeDuration` | `muted` has no documented difference from `ignored`; the other two are not status details | Offering two spellings of one meaning |
| One ignore condition per change | The docs describe each condition on its own; what a combination means is undocumented | A struct of optional conditions |
| Zero refused | SDK policy: the docs give zero no meaning, and it would read as "no condition" | Sending it |
| Response decoded as `Issue` | The ticket says the update returns the issue; the page shows a 200 code without a body | Leaving the output untyped |
| Separate bucket for the update | Limits are per "caller and endpoint", and the update is its own reference endpoint | Sharing the retrieve bucket (same URL): more conservative, but it couples reads to writes. Neither choice is verified live. |
| Retry 429 only | The rate-limit page says requests over the limits are rejected | Retrying 502/503/504, which may have been applied |
| No answer means `Uncertain` | `Delivery::Unknown` (AUTHOR_GUIDE §5.11b) and the ticket | `Error::Concord`/`RetryStopped`, which hide the ambiguity |
| A 5xx is `Provider`, not `Uncertain` | Keeps the class and typed document for every HTTP answer; the README tells callers to re-read | Folding 5xx into `Uncertain` (the guide's sample rule) |
| A final non-429 answer is not reported as `Deadline` (`answered_write`) | `receive_with` leaves `stop = Deadline` when expiry is noticed right after a `Done` classification, which would hide an applied update | Keeping the reads' conservative stop |

## Every file and location edited

- `src/update.rs` (new, 144 lines): `StatusUpdate`, `Ignore`, `MAX_IGNORE_WINDOW_MINUTES`,
  the checked constructors and the private `Wire` serializer.
- `src/lib.rs`: `mod update` and its re-export; the client doc line; the `PUT update_status`
  declaration in `scope issue`.
- `src/error.rs`:
  - `HttpClass::RateLimited` and `HttpClass::Server` docs;
  - the `RetryStopped` doc;
  - the new `InvalidUpdate` and `Uncertain` variants;
  - two `Display` arms.
- `src/policy.rs`:
  - the `SendPolicy` doc ("four" became "five");
  - the limiter capacity, now `keys().len()` instead of the literal `4`;
  - `keys()`: array length and the new key;
  - the `write` flag;
  - the classifier's `Err` arm;
  - the retryable-status match;
  - the `Uncertain` mapping;
  - the `answered_write` guard.
- `README.md`: the title, the intro, the new "Status updates" section with a compiled `no_run`
  example, "five buckets", the retry sentence, the exclusions and the record pointer.
- `Cargo.toml`: the description. `PROVENANCE.md`: an appended maintenance section.
- New: `fixtures/ignored.json`, `tests/update.rs` (8 regression tests) and
  `tests/status_update_use.rs` (consumer workflow).
- Unchanged: `tests/first_use.rs`, `protocol.rs`, `workflows.rs`, `support/mod.rs`,
  `FRICTION.md`, `FIRST_USE.md`, the root `Cargo.toml`, `Cargo.lock`, all shared crates.
  `FRICTION.md` item 7 and its capability table still say "four keys/GETs": they record the
  authoring state.

### Facts that had to change in several places

1. **Which endpoints the policy admits.** These had to change together:
   - the `api!` declaration;
   - the `keys()` list and its array length;
   - the limiter capacity, formerly a separate literal `4`;
   - the prose in the `SendPolicy` rustdoc and the README.

   Separately, the paged reads are selected by index (`keys()[0]`, `keys()[2]`). Appending the
   new key kept those indices valid.
2. **Which operations may be repeated.** Four code sites in `policy.rs`: the `Err` arm, the
   status match, the `Uncertain` mapping and `answered_write`. The same rule is also written in
   the `HttpClass`/`Error` docs, the endpoint doc comment and two README paragraphs.
3. **The wire vocabulary.** The field names of the `Wire` struct, the README table and the
   tests' literal JSON. The tests stay independent of the SDK on purpose.
4. **The 7-day maximum.** The constant, the README and the boundary test.

## Did the compiler or the tests catch an omitted edit?

I applied each mutation to a copy, ran `cargo test --locked --offline --no-fail-fast`, then
restored the file and verified its hash. Script and logs are in
`target/tmp/mutants/` (not part of the package).

| Omitted or wrong edit | Caught by |
| --- | --- |
| New key removed from `keys()`, array length left at 5 | **Compiler** (E0308) |
| New key removed and the length set back to 4 | Tests only, at runtime: `Configuration("unsupported operation")` fails all 8 update tests and the consumer test |
| Limiter capacity left at the old literal `4` | Tests only: `SendPolicy::default()` panics with `Admission(Capacity)`, so all 47 policy-building tests fail |
| Update key inserted first (shifts the paged indices) | Tests only: 13 existing pagination tests fail |
| `write` always false (update treated as a read) | Tests: 3 |
| No `Uncertain` mapping | Tests: 2 |
| `null` written for an absent detail | Tests: 2 |
| Window bound off by one (`..` for `..=`) | Tests: 2 |
| 502–504 retried for the update | Tests: 1 |
| Zero check removed | Tests: 1 |
| `answered_write` guard removed | **Not caught.** The race (expiry noticed right after a `Done` classification) cannot be reproduced deterministically: the policy uses the wall clock (`Instant`), not an injected one. |

## Remaining generic duplication

- **The operation list is restated in `policy.rs`.** It repeats the `api!` endpoints, and nothing
  generated lets a per-endpoint policy enumerate a client's operations. Forgetting one fails only
  at runtime, in tests that call that endpoint.
- **The limiter's key-storage capacity is a separate number** from the key set. It is now derived
  here, but the API still asks for both.
- **Write classification over `receive_with` is hand-assembled.** The SDK combines `Delivery`
  (the guide gives a rule for a final error), `ReceiveReport::no_response` and
  `completed < attempts` (deadline cancellation). Any SDK with writes will rewrite these ~15 lines.
- **The stop reason can mislead.** `ReceiveRun` reports `Deadline` for a run whose last completed
  head was classified `Done`, so each SDK must inspect the outcome itself.
- **`StopReason` is still compared through `Debug` in tests**, because the SDK does not re-export
  it (first-consumer incident 7).

## Provider-specific responsibilities (stay in this SDK)

- The status vocabulary, the ignore details and where they are placed in the body.
- The 7-day window maximum, and the zero-value policy.
- 429 as a rejection; 5xx and no-answer as possibly applied; the "re-read the issue" guidance.
- Treating the update as its own rate-limit endpoint.
- Quota header interpretation (unchanged).
- The issue response model.

## Candidate Concord improvements (recorded only; no shared crate edited)

1. **`concord-retry`:** after a `Done`/`Stop` classification, record that stop reason before
   checking expiry, or record the last decision in `ReceiveReport`. A completed final answer would
   then never read as `Deadline`. Smallest change: move the `stop` assignment before the
   post-classification expiry check.
2. **`concord-macros`:** a generated `Client::operations() -> &'static [Operation]`, so a
   per-endpoint policy can be built from the declaration. A forgotten endpoint would then be a
   non-issue or a test-time assertion, not a runtime refusal.
3. **`concord-ratelimit`:** `Limiter::with_keys(iter, clock)`, or capacity derived from declared
   budgets. The capacity would then stop being a separate fact.
4. **Docs or `concord-retry`:** a compiled recipe or helper for uncertain writes inside
   `receive_with`. For example `ReceiveReport::unanswered_dispatch()`, true when
   `no_response` holds or `completed < attempts`, alongside AUTHOR_GUIDE §5.11b.

## Validation record (worktree root, `CARGO_TARGET_DIR=<worktree>/target`, `CARGO_BUILD_JOBS=4`)

- Baseline before any edit: `cargo test --locked --offline -p concord-sdk-sentry` passed
  9 + 14 + 15 + 1 doctest.
- After the change, the same command passed:

  | Suite | Tests |
  | --- | --- |
  | `first_use` | 9 |
  | `protocol` | 14 |
  | `workflows` | 15 |
  | `update` | 8 |
  | `status_update_use` | 1 |
  | doctests (the README's two examples) | 2 |

  It passed 4 consecutive runs, plus one `--test-threads=1` run of the new suites.
- `cargo clippy --locked --offline -p concord-sdk-sentry --all-targets -- -D warnings`: clean.
- `RUSTDOCFLAGS="-D warnings" cargo doc --locked --offline -p concord-sdk-sentry --no-deps`: clean.
- `cargo fmt -p concord-sdk-sentry --check` and `git diff --check`: clean.
- `git diff` shows the root `Cargo.toml`, `Cargo.lock` and the earlier tests unchanged.

## Not verified

Against the live service:
- the 200 body shape;
- whether Sentry accepts the top-level `ignore*` fields;
- whether PUT and GET on one path share a quota.

Also:
- the `answered_write` path, which has no test;
- cross-process coordination, which is unchanged and still not provided.
