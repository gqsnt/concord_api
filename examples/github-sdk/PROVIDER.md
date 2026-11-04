# GitHub REST API: pinned version, sources, scope

This SDK is a **bounded slice** of the GitHub REST API for two workflows (GH-A activity audit,
GH-B workflow-artifact retrieval). It is not a complete GitHub SDK.

## Pinned version

| | |
|---|---|
| REST API version | `2026-03-10`, sent as `X-GitHub-Api-Version` on every request (`ApiVersion::pinned()`) |
| Media type | `Accept: application/vnd.github+json` on every request |
| `User-Agent` | `concord-example-github-sdk` unless the caller sets another (`GitHubContext::set_user_agent`); GitHub rejects requests without one |
| Authentication | `Authorization: Bearer <token>` (`#[sensitive]`) |
| Base | `https://api.github.com` (`GitHubContext::with_base_url` for GitHub Enterprise Server's `https://HOST/api/v3` or a test server) |

Version facts, from the API-versions page retrieved **2026-11-04** ([V1]): the supported versions
are `2026-03-10` and `2022-11-28`; a request **without** the header uses `2022-11-28`, so the SDK
always sends one; an unsupported version is answered `410 Gone`. The handoff's observation
(`2026-03-10`) matched.

## Schema identity

| | |
|---|---|
| Description | [github/rest-api-description][O1] (MIT) |
| Commit | `87267030f4b1cb33c3b076e4999f3da2e1290450` (2026-11-04T20:26:34Z, "Update OpenAPI 3.1 Descriptions") |
| File | `descriptions/api.github.com/api.github.com.2026-03-10.json`, `info.version` 1.1.4 |
| sha256 | `723661e4ec71d6d26a0d8bd831b11b3e99771e1561a135848bc90be0f7022445` |
| Retrieved | 2026-11-04 (the description file itself is not vendored; 18 of its examples are, under `examples/github-app/tests/fixtures/official/`) |

The models were written from that file's schemas (`full-repository`, `issue`, `issue-comment`,
`pull-request-simple`, `pull-request`, `pull-request-review`, `diff-entry`, `label`, `workflow`,
`workflow-run`, `job`, `artifact`) and the operation parameters, and are checked by decoding the
description's own examples (`tests/official_examples.rs`).

## Documentation used (all retrieved 2026-11-04)

| Ref | Page | Facts taken |
|---|---|---|
| [G1] | Using pagination in the REST API | `Link` with `prev`/`next`/`last`/`first`; `link` omitted when everything fits one page; `prev` omitted on the first page; `last` omitted when it cannot be calculated; follow the URLs, do not build them; `page`, `before`/`after` or `since` styles; `per_page` max 100, silently clamped |
| [G2] | Best practices for using the REST API | serial rather than concurrent requests; conditional requests with `etag`/`if-none-match` (`304` does not count against the primary limit when authorized); follow redirects (301 permanent, 302/307 temporary); don't parse URLs; `retry-after`, `x-ratelimit-remaining`/`reset` |
| [G3] | REST API endpoints for artifacts | list (repo, per run with `name` and `direction`), get, download: `GET .../artifacts/{id}/{archive_format}` (`zip` only), `302` with `Location` that expires after 1 minute, `410` listed |
| [RL] | Rate limits for the REST API | `x-ratelimit-limit/remaining/used/reset/resource`; primary and secondary limits are `403` or `429`; `retry-after` |
| [GS] | Getting started with the REST API | `User-Agent` required (`403` for an invalid one); `Authorization: Bearer` |
| [BC] | Breaking changes (2026-03-10) | singular `assignee` removed from issues/pull requests; `merge_commit_sha` removed; `has_downloads` removed from repositories |
| [ISS] [COM] [PUL] [REV] [WFR] [JOB] [REP] | issues, issue comments, pulls, reviews, workflow runs, workflow jobs, repositories reference pages | parameters, enums, status codes (cross-checked with the description file) |

## Supported operations (23)

| Family | Operation id | Method and path | Generated |
|---|---|---|---|
| Repository | `repos/get` | `GET /repos/{owner}/{repo}` (conditional) | `GitHubRepo::get` |
| Repository | `issues/list-labels-for-repo` | `GET .../labels` | `GitHubRepo::labels` |
| Issues | `issues/list-for-repo` | `GET .../issues` (milestone, state, assignee, creator, mentioned, labels, sort, direction, since, per_page) | `GitHubRepo::issues`, `RepoContext::issues(&IssueFilter)` |
| Issues | `issues/get` | `GET .../issues/{number}` (conditional) | `GitHubRepo::issue` |
| Issues | `issues/list-comments` | `GET .../issues/{number}/comments` (since, per_page) | `GitHubRepo::issue_comments` |
| Issues | `issues/list-comments-for-repo` | `GET .../issues/comments` (sort, direction, since, per_page) | `GitHubRepo::repo_comments` |
| Issues | `issues/get-comment` | `GET .../issues/comments/{comment_id}` | `GitHubRepo::comment` |
| Pull requests | `pulls/list` | `GET .../pulls` (state, head, base, sort, direction, per_page) | `GitHubRepo::pulls` |
| Pull requests | `pulls/get` | `GET .../pulls/{number}` (conditional) | `GitHubRepo::pull` |
| Pull requests | `pulls/list-reviews` | `GET .../pulls/{number}/reviews` | `GitHubRepo::pull_reviews` |
| Pull requests | `pulls/get-review` | `GET .../pulls/{number}/reviews/{review_id}` | `GitHubRepo::pull_review` |
| Pull requests | `pulls/list-files` | `GET .../pulls/{number}/files` | `GitHubRepo::pull_files` |
| Actions | `actions/list-repo-workflows` | `GET .../actions/workflows` | `GitHubRepoActions::workflows` |
| Actions | `actions/list-workflow-runs-for-repo` | `GET .../actions/runs` (actor, branch, event, status, created, exclude_pull_requests, check_suite_id, head_sha, per_page) | `GitHubRepoActions::runs`, `RepoContext::runs(&RunFilter)` |
| Actions | `actions/list-workflow-runs` | `GET .../actions/workflows/{workflow}/runs` | `GitHubRepoActions::workflow_runs` |
| Actions | `actions/get-workflow-run` | `GET .../actions/runs/{run_id}` | `GitHubRepoActions::run` |
| Actions | `actions/list-jobs-for-workflow-run` | `GET .../actions/runs/{run_id}/jobs` (filter) | `GitHubRepoActions::run_jobs` |
| Actions | `actions/get-job-for-workflow-run` | `GET .../actions/jobs/{job_id}` | `GitHubRepoActions::job` |
| Actions | `actions/list-artifacts-for-repo` | `GET .../actions/artifacts` (name) | `GitHubRepoActions::artifacts` |
| Actions | `actions/list-workflow-run-artifacts` | `GET .../actions/runs/{run_id}/artifacts` (name, direction) | `GitHubRepoActions::run_artifacts` |
| Actions | `actions/get-artifact` | `GET .../actions/artifacts/{artifact_id}` | `GitHubRepoActions::artifact` |
| Actions | `actions/download-artifact` | `GET .../actions/artifacts/{artifact_id}/zip` → `302` | `GitHubRepoActions::download_artifact` + `download::{locate, stream_to}` |

Every list operation has a page factory (`RepoContext::*_pages`, `paging::LinkPages`) for
`concord-pagination`. Operations the SDK does not declare can be built from the same context
(`RepoContext::request`, see `tests/operations.rs::undeclared_operation_through_the_context`).

## Deliberately unsupported

- **Writes** of any kind (creating/updating issues, comments, pull requests, reviews; deleting
  artifacts; re-running or dispatching workflows). The workflows are read-only.
- **The `page` query parameter.** GitHub says to follow `Link` instead of building page queries;
  the SDK never computes a page number. (`RepoContext::request` can still reach anything.)
- `type` and `issue_field_values` issue filters, `anon` contributors, workflow-run attempts, check
  runs, job/step log downloads (also `302`; would reuse `download`), timeline and events, Search,
  GraphQL.
- Alternative media types (`body_html`/`body_text`, `.diff`/`.patch` Accept values).
- `If-Modified-Since` (only `If-None-Match` is declared).
- GitHub App JWT and installation-token exchange (the caller supplies a token).
- Rate-limit waiting and retries: the metadata is reported (`ResponseMeta::rate_limit`,
  `is_rate_limited`); pacing is the application's policy.

## Discrepancies found and how they are resolved

| # | Observation | Resolution |
|---|---|---|
| D1 | The pinned `job` schema lists `created_at` as required, but the description's own `job` and `job-paginated` examples omit it (observed: the first decoding run failed on them). | `Job::created_at` is `Option<Timestamp>` with a default. |
| D2 | 2026-03-10 removed the singular `assignee` ([BC]); the description's `issue` examples still carry it. | Not modelled (`assignees` only); unknown fields are ignored. |
| D3 | `pull-request-review.state` has no enumeration in the description (example `CHANGES_REQUESTED`). | `ReviewState` names `APPROVED`, `CHANGES_REQUESTED`, `COMMENTED`, `DISMISSED`, `PENDING` and keeps anything else as `Unknown(text)`; every documented enum does the same. |
| D4 | The artifact-download `410` is listed without a meaning ([G3]). | Treated as "bytes no longer available" (`Located::Gone`, GH-B `Expired { signal: Gone }`); this is an inference, also consistent with the artifact's `expired` flag. |
| D5 | `Link` targets may address the collection by repository id (`/repositories/1300192/issues?page=2` in [G1]'s example) rather than by owner/name. | Links are followed as sent; the check is origin plus base-path prefix (`GitHubContext::is_api_url`), not path equality with the first request. |
| D6 | `per_page` above 100 is silently clamped ([G1]). | `PerPage::new` refuses 0 and values above 100. |
| D7 | The repository reference page shows `created_at`/`updated_at`/`pushed_at` as nullable; the `full-repository` schema lists them as required strings. | `Option<Timestamp>` (the more permissive reading). |
| D8 | `actions/list-workflow-runs-for-repo`'s `status` mixes statuses and conclusions in one enumeration. | One `RunStatus` parameter enum with all 14 documented values; the run model keeps `status` and `conclusion` separately. |

## Evidence status

- Offline: every operation's request and its decoding of the official example are tested
  (`cargo test -p concord-example-github-sdk`); both workflows run against loopback stand-ins
  (`cargo test -p concord-example-github-app`).
- Live: **NOT_RUN**. No credentials were provided; `gh_a`/`gh_b` print `NOT_RUN` without them.
  Nothing here is a capture of a real GitHub response.

[V1]: https://docs.github.com/en/rest/about-the-rest-api/api-versions
[O1]: https://github.com/github/rest-api-description
[G1]: https://docs.github.com/en/rest/using-the-rest-api/using-pagination-in-the-rest-api
[G2]: https://docs.github.com/en/rest/using-the-rest-api/best-practices-for-using-the-rest-api
[G3]: https://docs.github.com/en/rest/actions/artifacts
[RL]: https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api
[GS]: https://docs.github.com/en/rest/using-the-rest-api/getting-started-with-the-rest-api
[BC]: https://docs.github.com/en/rest/about-the-rest-api/breaking-changes
[ISS]: https://docs.github.com/en/rest/issues/issues
[COM]: https://docs.github.com/en/rest/issues/comments
[PUL]: https://docs.github.com/en/rest/pulls/pulls
[REV]: https://docs.github.com/en/rest/pulls/reviews
[WFR]: https://docs.github.com/en/rest/actions/workflow-runs
[JOB]: https://docs.github.com/en/rest/actions/workflow-jobs
[REP]: https://docs.github.com/en/rest/repos/repos
