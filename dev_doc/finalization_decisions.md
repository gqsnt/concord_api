# Finalization decisions

Each entry records a judgment not fixed by the finalization plan.

## Phase 0

- Keep `docs/SUMMARY.md` as the Phase 0 canonical documentation inventory because the repository already enforces it as a complete list of every Markdown page under `docs/`.
- Treat the missing `docs/runtime_config.md` defect as inapplicable to this revision because no Markdown file references that path and the current production-configuration material is split between `docs/features/production-http.md` and `docs/recipes/production-configuration.md`.
- Replace the mdBook hierarchy in Phase 2 with `docs/quick_start.md`, `docs/spec.md`, `docs/runtime.md`, `docs/auth.md`, `docs/bodies_and_responses.md`, `docs/pagination.md`, `docs/retry_and_rate_limit.md`, `docs/generated_client.md`, `docs/advanced_endpoints.md`, `docs/customization.md`, and `docs/design_invariants.md`; these topic files preserve the current unique user-facing subjects without retaining the guide/reference duplication.
- Consolidate compiler, workspace, testing, and extension-maintenance material into `dev_doc/internals.md` in Phase 2 because it is maintainer material rather than part of the user specification.
- Retain `dev_doc/doc_inventory.md` as the audit record for the pre-finalization documentation graph.
- Delete `CHANGELOG.md`, `fixtures/README.md`, and `tools/perf/README.md` after their current-state facts are folded into maintainer documentation because history and peripheral instructions are outside the frozen user documentation set.
- Remove the mdBook-specific configuration and generation path after Phase 2 because the final documentation set is raw Markdown and the project has no website deliverable.

## Phase 1

- Preserve the current `credentials { ... }` and `rate_limits { ... }` declaration blocks because this revision has no `auth { ... }`, `policies { ... }`, `behaviors { ... }`, or `defaults { ... }` configuration dialect and those prompt names do not map one-to-one onto the current language.
- Keep client `auth ...;` and `rate_limit ...` clauses because they are attachment clauses in the current sole dialect, not flat declarations of credential or policy profiles.
- Reject flat `secret` and `credential` declarations with a focused direction to `credentials { ... }` because removing their parser paths is already complete in this revision.
- Reject both `default { ... }` and `defaults { ... }` blocks with directions to value-level defaults because the current language expresses defaults only as `name: Type = VALUE`.
- Keep retry and behavior configuration out of the DSL because retry is already installed through the generated builder and behavior profiles do not exist.
- Treat `auth certificate` as ghost syntax and direct users to `configure_http` because application client-certificate material is supported end to end by `HttpConfig::identity_pem`, but it is runtime configuration rather than an authentication attachment.
- Use trybuild `.stderr` files under `crates/concord/tests/ui/` as the Phase 1 output snapshots because this revision has no `concord_macros/tests/snapshots/` directory and trybuild is the maintained diagnostic snapshot mechanism.

## Phase 2

- Consolidate the 75-page mdBook hierarchy into the eleven user and maintainer files selected in Phase 0 because the hierarchy repeated grammar, runtime, and feature rules across guide, feature, recipe, extension, reference, and contributor pages.
- Keep body/response delivery, authentication runtime behavior, pagination execution, retry/rate limiting, generated Rust, runtime-defined endpoints, and extension traits as topic files because each owns material not expressible as DSL grammar.
- Move compiler pipeline, workspace ownership, testing, security review, and performance review into `dev_doc/internals.md` because these rules apply to maintainers rather than API users.
- Remove mdBook configuration, theme, commands, dependency, and workflow steps because the final documentation is raw Markdown and the project has no website deliverable.
- Remove the pre-finalization changelog and peripheral fixture/performance READMEs after folding their current operational facts into `dev_doc/internals.md` because the frozen private project does not publish release history or peripheral manuals.
- Keep the existing CI workflow while removing only website steps because the prompt forbids adding CI setup but does not direct deletion of the repository's existing validation workflow.
