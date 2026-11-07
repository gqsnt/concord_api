# Independent review: fixes for the b23-review2 findings (between waves 2 and 3)

Read-only review. The working directory is a git worktree of the Concord workspace on
`sdk50/integration` at `20517134`. Review `git show 20517134` (one commit). Do not build or run
tests; read code, tests and docs. No external service. The findings it claims to fix are in
`/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/roles/b23-review2-sol/last.md`.

Check, do not trust:
1. `with_output()` (crates/concord-macros/src/emit.rs): `syn::visit_mut` replaces `Type::Infer`
   only, skips every expression and lifetime and fn parameter names, and substitutes a type macro's
   tokens under every delimiter. Any type position where `_` is still missed or wrongly replaced
   (qualified paths `<_ as Trait>::X`, `impl Trait<_>`, `dyn Fn(_) -> _`, associated bindings,
   parenthesized/grouped types, a `_` forwarded as a `ty` fragment, nested macros)? Does
   re-emitting the parsed type lose spans, hygiene or the invisible groups of forwarded
   fragments? Tests: the unit test `a_send_template_replaces_type_placeholders_only`,
   checks/core-concepts/tests/w2_declared_send_result.rs.
2. The UI cases `crates/concord/tests/ui/send_declared_result_{mismatch,coercion}` now use a
   drain endpoint: do they still prove what they claim, under every feature set the gate runs?
3. Discord (`examples/discord-rest-sdk`: limiter.rs `failed_before_head`, error.rs, README) and
   Zendesk (`examples/zendesk-support-sdk`: policy.rs, error.rs, README): every transport failure
   on a write is now uncertain. Is any write still reported as definitely not sent when it may
   have been performed, or any read now wrongly uncertain? Do the rewritten tests
   (`a_refused_connection_on_a_write_is_ambiguous`, `a_refused_connection_on_a_create_is_uncertain`)
   assert the new contract, and do the docs match?
4. AUTHOR_GUIDE §5.11 (field readers on an error's head) and §5.11b (when an SDK may keep a
   connect rule): accurate and compilable?

Output: findings most severe first (file:line, defect, concrete failure scenario, CONFIRMED by
reading / PLAUSIBLE, smallest fix), then claims found sound, then areas not covered.
