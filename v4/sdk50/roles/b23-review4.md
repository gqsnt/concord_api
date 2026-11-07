# Independent review: fixes for the b23-review3 findings (between waves 2 and 3)

Read-only review. The working directory is a git worktree of the Concord workspace on
`sdk50/integration` at `c3517aaf`. Review `git show c3517aaf` (one commit). Do not build or run
tests; read code, tests and docs. No external service. The findings it claims to fix are in
`/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/roles/b23-review3-sol/last.md`.

Check, do not trust:
1. `with_output()` (crates/concord-macros/src/emit.rs): a type macro's input is parsed as
   comma-separated types (`Punctuated<Type, Token![,]>::parse_terminated`), filled by the same
   structural rule, and re-emitted only when a placeholder was filled; any other grammar is kept.
   Does re-emitting a parsed macro input change what the macro receives (spacing-sensitive or
   hygiene-sensitive matchers, trailing commas, `$t:ty` vs `$t:tt` matchers, nested macros)? Is
   there a type position still missed or wrongly replaced? Tests: the unit test
   `a_send_template_replaces_type_placeholders_only`.
2. Discord (`examples/discord-rest-sdk`: README, src/api.rs) and Zendesk
   (`examples/zendesk-support-sdk`: README, src/lib.rs): with redirects and retries required off,
   is any remaining "refused"/"not sent" claim unsound? Is the README example valid code
   (reqwest 0.13.5 `redirect::Policy::none()`, `retry::never()`)?
3. `checks/core-concepts/tests/w2_delivery.rs::a_followed_redirect_answers_for_a_later_request`:
   does it witness what its comment claims?
4. AUTHOR_GUIDE §5.11b: accurate?

Report only real defects. Output: findings most severe first (file:line, defect, concrete failure
scenario, CONFIRMED by reading / PLAUSIBLE, smallest fix), then claims found sound, then areas not
covered.
