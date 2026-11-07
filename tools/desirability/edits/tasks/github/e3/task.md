# github / e3: optional query value on every list operation of a resource

**Provider changelog.** Both workflow-run list operations, `GET /repos/{owner}/{repo}/actions/runs`
and `GET /repos/{owner}/{repo}/actions/workflows/{workflow}/runs`, accept an optional
`triggering_actor` query value: the login that triggered the run's latest attempt. Unset,
nothing is sent, as before. Nothing else changed.

Public names the hidden check uses:
- `repo.actions().runs().set_triggering_actor(Login::new(..))` and
  `repo.actions().workflow_runs(workflow).set_triggering_actor(..)` send
  `triggering_actor=<login>`;
- the caller-facing filter of the run list carries it too: `RunFilter::triggering_actor:
  Option<Login>` (public field), applied by `RepoContext::runs(&filter)`.
