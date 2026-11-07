# Independent review: `send P -> Template;` no longer enters macro invocations

Read-only review. The working directory is a git worktree of the Concord workspace on
`sdk50/integration` at `14b9e1e0`. Review `git show 14b9e1e0` (one commit): `with_output()` in
crates/concord-macros/src/emit.rs, its unit test, checks/core-concepts/tests/w2_declared_send_result.rs,
the UI case crates/concord/tests/ui/send_template_macro_placeholder.{rs,stderr}, AUTHOR_GUIDE §5.13.
Do not build or run tests; read code. No external service. Earlier findings on this function:
`/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/roles/b23-review{,2,3,4,5}-sol/last.md`.

The claim: every `syn::Type::Infer` of the template becomes the endpoint's output; lifetimes, fn
parameter names, every expression and every macro invocation are left as written; the rest is
re-emitted from the parsed type and forwarded fragments keep their invisible groups. Check: is any
earlier finding reintroduced? Is there a type position where `_` is missed or a non-type `_`
replaced? Does the UI snapshot prove what its comment says, under every feature set? Is the guide
sentence accurate?

Report only real defects. Output: findings most severe first (file:line, defect, concrete failure
scenario, CONFIRMED by reading / PLAUSIBLE, smallest fix), then claims found sound, then areas not
covered.
