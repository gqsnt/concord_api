# Maintenance — SDK37 Strava: update an owned activity

Role: independent maintainer (did not write the SDK or its first consumer). Date: 2026-11-07.
Frozen baseline `c3fd4f1ee30c1727e26b285270f2cc8d8b694e77`; worktree HEAD at start `58da2ea2`
(clean tree). Live provider calls: **NOT_RUN**. Only `examples/strava-sdk/` changed; no shared
crate, check, script or other example was edited; `Cargo.toml`, `Cargo.lock` and
`tests/first_use.rs` are byte-identical to HEAD.

## Ticket

Callers update an activity they own (name, sport type, description, gear, commute and trainer)
with `PUT /activities/{id}`, setting only the fields they mean to change and getting the updated
activity back. A token without `activity:write` is a typed authorization error; a timed-out
update is uncertain (the caller re-reads) and never resent automatically; the update counts
against the same usage windows as other requests. Listing, retrieval and uploads stay unchanged.

## Prior exposure (what was read before starting, in order)

1. My role brief only.
2. `git status`, `git log -3`, the package file list.
3. This package's `src/*.rs`, `README.md`, `VALIDATION.md`, `PROVENANCE.md`,
   `tests/support/mod.rs`, `tests/workflows.rs`, `tests/protocol.rs` and parts of
   `tests/first_use.rs` (helpers, workflow 1, detail failures, 429 tests).
4. Official pages (below), then `docs/CHEATSHEET.md`, `docs/AUTHOR_GUIDE.md` §5.4 and
   §5.11b–§5.13, `crates/concord-core/src/response.rs` (`Operation`) and
   `crates/concord-core/src/error.rs` (`operation()`), the `examples/testserver` public
   function list.

`FRICTION.md` and `FIRST_USE.md` were read **after** the change was implemented and its tests
passed; they changed nothing in the implementation (see "Relation to first-use findings").
No other example, campaign, brief, record or worktree file was read.

## Provider sources (read 2026-11-07, read-only page fetches; no file downloaded)

| Page | Facts used |
|---|---|
| https://developers.strava.com/docs/reference/ — `updateActivityById`, `UpdatableActivity`, `UpdatableActivity_tags`, `SummaryActivity`, `DetailedActivity`, `Fault` | `PUT /activities/{id}` "Updates the given activity that is owned by the authenticated athlete"; body `UpdatableActivity`; 200 returns `DetailedActivity`; 4xx/5xx a `Fault`. Scope: "Requires activity:write", plus `activity:read_all` for Only Me activities. `gear_id`: `none` clears the gear. `type`: deprecated, ignored when `sport_type` is present. Tags: only included tags change; commute is set through the `Commute` tag rather than the legacy `commute` param, and the two cannot be combined. `description` exists only on `DetailedActivity`; `gear_id`, `commute`, `trainer` on both. |
| https://developers.strava.com/docs/authentication/ — scopes | `activity:write` allows editing activities the app can see; athletes may opt out of requested scopes; the token exchange returns the granted, space-delimited scope list; the refresh response carries no scope; **no status is documented for a request lacking a scope**. |
| https://developers.strava.com/docs/rate-limits/ | The non-upload limit applies to all endpoints except activity creation, uploads and activity media uploads, so an update is charged to the overall **and** the non-upload windows. |

Not stated by the pages, hence SDK choices recorded as such: the request media type (the SDK
sends JSON) and the provider's status for a missing scope.

## What changed for callers

- `api.activities().update(id, ActivityUpdate)` → `Result<Response<Activity>, Error>`.
  `ActivityUpdate::new()` plus `name`, `sport_type`, `description`, `gear_id`, `clear_gear`,
  `commute`, `trainer`. Only set fields are serialized. `clear_gear()` sends `"gear_id":"none"`;
  `commute(b)` sends `"tags":{"Commute":b}` and never the legacy `commute`; `type` is never sent.
  `Debug` names the set fields only.
- `ActivityUpdate::is_reflected_in(&Activity)`: the re-read check for an uncertain update, with
  the `none` gear sentinel handled; it is evidence, not proof (a later edit or normalization can
  also make it `false`), and the SDK never resends.
- `Activity` gains `description`, `gear_id`, `commute`, `trainer` (all `Option`, `None` when
  absent or `null`), so the update response and the re-read show the edited fields.
- `Error::MissingScope(&'static str)`: when the session's stored `Tokens.scope` is known and lacks
  `activity:write`, the update is refused before any token refresh, admission or dispatch. With
  unknown scopes (`None`) it is sent, and a provider rejection is the usual `Error::Http` with its
  status and `Fault` (no mapping of an undocumented status).
- Uncertainty: a timeout, a lost connection or an unreadable 2xx is `Error::Ambiguous`; a 5xx is
  `Error::Http` with `ambiguous == true`; `is_ambiguous()` covers all. One attempt only.
  `Display` for an uncertain update says to re-read; the upload text is unchanged.
- Quota: updates are admitted against, and their responses observed into, all four windows.

Compatibility for existing callers: list/get/upload requests on the wire are unchanged (the
existing 25 first-use and 22 author tests pass unmodified). Two source-level breaks are possible
for code this repository does not contain: an exhaustive `match` on `Error` must add
`MissingScope`, and an `Activity { .. }` struct literal or exhaustive destructuring must add the
four fields. One value-level change: a listed activity and its detail now differ in
`description` when the activity has one (summaries omit it), so `list_item == detail` no longer
holds for such activities; the first-use test that compares them uses description-less fixtures.

## Files and locations edited

| File | Location | Change |
|---|---|---|
| `src/update.rs` (new) | whole file | `ActivityUpdate`, `Tags`, `NO_GEAR`, builder, `is_reflected_in`, field-name `Debug` |
| `src/lib.rs` | `mod update` / `pub use` (l. 9, 15) | export |
| `src/lib.rs` | `Activity` (l. 26–51) | four optional fields, doc |
| `src/lib.rs` | `concord::api!` `scope activities` (l. 98–104) | `PUT update(id, body changes) -> Activity { path "{id}"; request Json; }` |
| `src/lib.rs` | `finish` (l. 143–162) | parameter `upload` → `uncertain` (5xx ambiguity, unreadable 2xx) |
| `src/auth.rs` | `Class`, `Class::of`, `granted` (l. 248–285) | new: every operation-keyed rule of the send policy in one place |
| `src/auth.rs` | `SendWith for Session::send` (l. 287–383) | scope pre-check; admission keys and observation keyed on `non_upload` (was `read`); `uncertain` passed to `from_call`/`finish`; retry delay from `Class` |
| `src/error.rs` | imports, `HttpFailure.ambiguous` doc, `Ambiguous` doc, new `MissingScope`, `from_call` parameter, `is_ambiguous` doc, `Display` (l. 2, 56, 75–81, 84, 92, 115–124) | uncertain-write wording, typed scope error, update-specific `Display` |
| `src/rate.rs` | `RateLimits` field docs, `observe` parameter (l. 36–39, 53, 62) | `read: bool` → `non_upload: bool`; docs say which requests charge which quota |
| `tests/protocol.rs` | appended (l. 527–) | five regression tests (below); no existing line changed |
| `tests/support/mod.rs` | l. 5 | `UPDATED` fixture constant |
| `tests/update_consumer.rs` (new) | whole file | four consumer tests (below) |
| `fixtures/activity-updated.json` (new) | — | synthetic updated activity |
| `README.md` | intro, scopes sentence, new update section with a compile-checked example, failure table (3 rows), admission sentence, exclusions | caller contract |
| `PROVENANCE.md` | appended "Maintenance" section | sources, dates, fixture, files read |
| `VALIDATION.md` | appended pointer | to this file's results |

### Facts that had to change in several places

- **"Which operations are uncertain writes"** (was `upload: bool` from one operation compare):
  `Class::of` (`uncertain`), the update branch of `Error`'s `Display` (a second
  `StravaActivitiesUpdate::operation()` compare in `error.rs`), and the docs of `Error::Ambiguous`,
  `is_ambiguous`, `HttpFailure.ambiguous`, the endpoint, and two README table rows.
- **"Which requests charge the non-upload windows"**: implicit before as `method == GET`, one
  variable that drove three rules (replay, admission keys, header observation). Split into
  `read` (replay) and `non_upload` (both quota uses) in `Class::of`; repeated in
  `RateLimits.read`'s doc, `observe`'s parameter and the README rate paragraph.
- **`activity:write`**: `Class::of` (`scope`), endpoint doc, README (twice), `MissingScope` doc.
  Uploads also need it, but per the ticket they keep their behavior (no local check).
- **The updatable field set**: four lists in `update.rs` that the compiler does not tie together
  (struct, builder, `is_reflected_in`, `Debug`), plus the four `Activity` fields, plus the README.
- **Strava spellings** `none` and `Commute`: `update.rs` only, plus README prose.

## Did the compiler or the tests catch an omitted edit?

First I added only the declaration and the models (endpoint, `ActivityUpdate`, `Activity`
fields). **It compiled with no error or warning, Clippy-clean, and all 50 existing tests
passed.** The generated `send()` routed the PUT through the existing policy, which classified it
by method (`GET` → read) and by comparing with `StravaUploadsCreate`. The new regression tests
then failed on that naive state (captured in `target/tmp/naive-failures.txt`):

| Behavior | Naive state (observed) |
|---|---|
| Granted scopes lack `activity:write` | expired token refreshed (`POST /oauth/token`), then the PUT dispatched; no typed error |
| Read reported an exhausted non-upload window | update admitted and sent (charged only to overall keys) |
| Update timeout | `Error::Concord(Transport)`, `is_ambiguous() == false` |
| Automatic resend | already absent (non-GET → `Replay::Never`); correct by accident of the method rule |

Mutation checks on the final code (each re-introduces one omitted edit; copies and results in
`target/tmp/mutations/`): **15 of 15 mutants were caught by tests, 0 by the compiler.**

| Mutant | Caught by |
|---|---|
| M1 quota class keyed on GET again | `activity_update_is_charged_to_the_non_upload_windows`, `update_is_gated_by_the_non_upload_window_that_reads_exhausted` |
| M2 update not uncertain | `activity_update_failures_after_dispatch_are_uncertain_and_never_resent`, `uncertain_update_is_resolved_by_rereading_never_by_an_automatic_resend` |
| M3 no scope requirement | `activity_update_without_granted_activity_write_is_refused_before_any_request`, `athlete_who_did_not_grant_write_gets_a_typed_error_and_reads_still_work` |
| M4 comma-delimited grants rejected | scope regression test — **survived at first** (only a negative comma case); a positive `read,activity:write` case was added |
| M5 observation skips non-upload windows for an update | non-upload windows regression (429 on an update must not become an unexplained day-long gate) |
| M6 admission skips non-upload keys for an update | both quota tests |
| M7 update 5xx not ambiguous | update uncertainty test (+ existing upload tests) |
| M8 legacy `commute` instead of the tag; M9 `clear_gear` sends `""`; M10 unset field sent as `null` | wire test and consumer edit test |
| M11 update `Display` falls back to upload text | update uncertainty test |
| M12 unreadable 2xx not uncertain | update uncertainty test (+ existing upload tests) |
| M13 status-poll retry delay lost when moved into `Class` | existing `status_retry_preserves_the_one_second_poll_interval` |
| M14/M15 `is_reflected_in` ignores description / trainer | **survived at first**; `update_reflection_checks_every_set_field_and_the_gear_sentinel` added |

Conclusion: every rule the ticket names is an operation-keyed policy decision outside the
declaration. Concord's macro checks the declaration (path, body format, output type) and the
send-policy result template, but nothing in the declaration states replay, uncertainty, quota
class or scope, so forgetting any of them compiles silently. Only tests written for each rule
caught the omissions.

## Tests added

`tests/protocol.rs` (regressions): `activity_update_sends_only_the_set_fields_as_json` (method,
path, JSON media type, three exact bodies, no legacy `commute`/`type`, value-free `Debug`),
`activity_update_is_charged_to_the_non_upload_windows` (read-exhausted window gates the update
but not an upload; an update 429 that exhausts the non-upload short window gates reads, not
uploads), `activity_update_failures_after_dispatch_are_uncertain_and_never_resent` (timeout,
hang-up, 503, unreadable 200; one PUT each), `activity_update_without_granted_activity_write_is_refused_before_any_request`
(space and comma lists, expired token not refreshed, reads/uploads unchanged, unknown or granted
scopes sent once, 401 kept), `update_reflection_checks_every_set_field_and_the_gear_sentinel`.

`tests/update_consumer.rs` (application view, public API only): list → edit → returned
activity → clear gear; uncertain update resolved by re-reading (applied: no resend; lost: the
application resends explicitly); athlete who unchecked `activity:write` gets
`MissingScope("activity:write")` and reads still work; an update gated by the non-upload window a
listing exhausted while an upload still goes out.

## Validation (observed, local, 2026-11-07)

| Command | Result |
|---|---|
| `cargo test --locked --offline -p concord-sdk-strava` (before) | 25 first_use, 12 protocol, 10 workflows, 3 doctests: PASS |
| `cargo test --locked --offline -p concord-sdk-strava` (after) | 25 first_use, 17 protocol, 4 update_consumer, 10 workflows, 4 doctests (60): PASS |
| `cargo test … --test protocol --test update_consumer`, 5 repetitions | PASS each time |
| `cargo clippy --locked --offline -p concord-sdk-strava --all-targets -- -D warnings` | PASS, no suppressions |
| `RUSTDOCFLAGS="-D warnings" cargo doc --locked --offline -p concord-sdk-strava --no-deps` | PASS |
| `cargo fmt -p concord-sdk-strava -- --check`, `git diff --check` | PASS |
| `git diff --quiet -- Cargo.toml Cargo.lock examples/strava-sdk/tests/first_use.rs` | unchanged; 0 removed lines under `tests/` |

All builds used `CARGO_TARGET_DIR=<worktree>/target` (absolute), `CARGO_BUILD_JOBS=4`, one
cargo command at a time. No new dependency. Intermediate diagnostics, all package-local: E0599
(`Error::MissingScope` did not exist yet while the regression tests were written first); two
Clippy `clone_on_copy` and one `needless_borrows_for_generic_args` in the new tests; rustfmt
reflow. Not verified: live Strava behavior (JSON acceptance of the body, the `Commute` tag, the
real status of a missing-scope rejection).

## Remaining generic duplication

- Operation identity compared by hand: `Class::of` and `Error`'s `Display` each compare
  `E::operation()` with generated `…::operation()` constants. A third write would add a third
  compare in both places.
- The method-to-replay rule (`GET` → replay) and the delivery-to-uncertainty rule
  (`Delivery != NotSent` for writes) are the same generic shapes the author guide shows (§5.11b);
  each SDK re-derives them.
- A partial-update body is a hand-written struct of `Option`s with `skip_serializing_if` on every
  field, a builder per field, a field-name `Debug`, and a reflection check: four parallel lists.
- Scope-string parsing (`granted`) and a "required scope per operation" table are generic OAuth
  mechanics written locally.

## Provider-specific responsibilities (stay in the SDK)

`gear_id` `none`; `Commute` as a tag and never with the legacy field; `type` never sent; which
endpoints are exempt from the non-upload windows; `activity:write` for updates and
`activity:read_all` for Only Me activities (enforced only by the provider); the meaning of the
`Fault` document; treating an update as never resendable even though PUT is idempotent in HTTP
terms (a resend can overwrite a concurrent edit and spends quota); the decision not to map an
undocumented missing-scope status.

## Candidate Concord improvements (recorded, not applied)

1. **Per-endpoint policy facts in the declaration**, e.g. an attribute or statement read by the
   send policy (`#[write(uncertain)]`, `#[quota("non_upload")]`, `#[scope("activity:write")]`)
   exposed as `Operation` metadata. Today every such rule is a hand comparison that compiles
   silently when forgotten (15/15 mutants were invisible to the compiler). Even a single
   declared, typed `class` per endpoint that the policy must match exhaustively would make a new
   endpoint without a class a compile error.
2. **A delivery-aware write classifier** in `concord` (the §5.11b example promoted to an API,
   parameterised by the SDK's 5xx/unreadable-2xx rules), so SDKs stop rewriting it.
3. **A partial-update body helper** (a derive for "only set fields", with field-name `Debug`), or
   at least a documented recipe; it would remove three of the four parallel field lists.
4. **`#[non_exhaustive]` guidance** for SDK error enums and response models in the author guide:
   this maintenance had to add an `Error` variant and `Activity` fields, both source-breaking for
   exhaustive callers.
5. A small OAuth scope-set helper in `concord-auth` (parse space/comma lists, `contains`), keeping
   the per-operation requirement in the SDK.

## Relation to first-use findings (read afterwards)

FIRST_USE incident 9 ("three spellings of ambiguous") applies to updates too: an update uses the
same `Error::Ambiguous` / `HttpFailure.ambiguous` / `is_ambiguous()` trio, and the README example
uses `is_ambiguous()` as the single check. Incident 5 (no re-export of `concord_ratelimit`) also
applies: my consumer test names `concord_ratelimit::Error::Gated` exactly as the first-use test
does. Incident 12 (uploads page documents `description`/`trainer`/`commute` for uploads) remains
out of scope. FRICTION F4/F6 (retry/admission composition and typed errors written per SDK)
describe the same place this change had to edit.
