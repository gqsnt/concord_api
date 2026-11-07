# Independent follow-up review: the b23 review fixes and the SDK adoptions (between waves 2 and 3)

Read-only review. The working directory is a git worktree of the Concord workspace on
`sdk50/integration` at `4870f864`. Do not build or run tests; read code, tests and docs. No external
service. The previous review report is `/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/roles/b23-review-sol/last.md`
(six findings). Review three changes:

A. `git show 95ac36de`: the fixes for those six findings. For each finding, is it fixed, and did the
   fix introduce a new defect? In particular:
   - `Error::delivery()` (crates/concord-core/src/error.rs): every headless transport failure is now
     `Unknown`; `NotSent` only for Build/HeaderConflict/Encode. Can any `NotSent` error be produced
     after a request byte may have left the process? Is `Answered` sound for Status/Metadata/
     Decode/Limit and for a transport failure with a retained head? Tests:
     checks/core-concepts/tests/w2_delivery.rs (including the redirect witness).
   - `send P -> Template;` (crates/concord-macros/src/{syntax,emit}.rs): the equality bound
     `SendWith<Out, Output = Declared>`, `with_output()` (type `_` only; lifetimes `'_`, brace groups
     and unchanged groups kept), `parse_type` for forwarded fragments. Tests:
     checks/core-concepts/tests/w2_declared_send_result.rs, crates/concord/tests/ui/send_declared_result_*.
   - AUTHOR_GUIDE §5.11, §5.11b, §5.13, CHEATSHEET rows, generated `query(..)` docs (`type_text`).
B. `git show 0e4a245e`: ten earlier SDKs adopt the declared send result and `concord::field`
   readers. Any behavior change for callers (a stricter or looser header parse, a different
   generated `send()` result type, a lost error case)? Check against each SDK's tests.
C. `git diff 3ff63633 4870f864`: five wave-2 SDKs (Postmark, Mailgun, Zendesk Support, Intercom,
   HubSpot CRM) adopt the declared result, `concord::field` and, in HubSpot, `delivery()`. The
   commit messages of `86cd55df` and `a46f490a` say what was kept and reverted, and that three
   tests were rewritten to pin "a refused connection on a write stays uncertain". Is any
   classification now unsafe (a write reported as definitely not sent when it may have been
   performed)? Do the README/rustdoc claims match the code? Is any existing assertion weakened
   (compare with `162fc869`)?

Output: findings most severe first (file:line, defect, concrete failure scenario, CONFIRMED by
reading / PLAUSIBLE, smallest fix), then claims found sound, then areas not covered.
