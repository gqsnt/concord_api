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
