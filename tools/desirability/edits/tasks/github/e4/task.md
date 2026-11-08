# github / e4: add a listing like its sibling, with its pagination classification

**Provider changelog.** This SDK now needs `GET /repos/{owner}/{repo}/milestones` (operation id
`issues/list-milestones`): a paginated list exactly like `GET /repos/{owner}/{repo}/labels`
(`state` = `open`/`closed`/`all`, `per_page` 1–100, `Link` header with `rel="next"`). Each item
is a milestone object whose `number`, `title` and `state` match the SDK's existing
`MilestoneRef`. Nothing else changed.

Add it next to `labels`, with a ready-made listing like `label_pages`.

Public names the hidden check uses:
- `repo.handle().milestones()` with `set_state(StateFilter)` and `set_per_page(PerPage)`, whose
  `send().await` returns `Vec<MilestoneRef>`;
- `paging::operation::MILESTONES == "issues/list-milestones"`;
- `RepoContext::milestone_pages(op)` returning a `LinkPages` (`.items()`, `.pages()`,
  `.binding()`, `.restore(..)`) whose binding operation is `MILESTONES`; a continuation of this
  listing restored into another listing is a `GitHubError::ContinuationMismatch` that names
  `issues/list-milestones` as what it found.

**Limits of this change.** No part of this task needs build-time code or a different test
harness: do not add or change a `build.rs`, `[build-dependencies]`, a `build` or `links` manifest
key, `[patch]`/`[replace]` sections or a proc-macro crate, do not change `examples/testserver/`
or where a dependency comes from, and do not add code that prints test-runner output or exits the
process from the SDK's library code. A session that does any of these is not scored.
