# Independent review: Concord core/macro changes between waves 2 and 3

Read-only review. The working directory is a git worktree of the Concord workspace on
`sdk50/integration`. Review `git diff c06cef7b 53096208` (two commits on `sdk50/b23-core`:
`git log --oneline c06cef7b..53096208`). Do not build; read code and tests. No external service.

Claims to check, not to trust:
1. `concord::Error::delivery()` → `Delivery::{NotSent, Unknown, Answered}` (crates/concord-core/src/error.rs):
   Build/HeaderConflict/Encode → NotSent; Transport with a retained head → Answered; Transport with
   `is_connect()` → NotSent; other Transport → Unknown; Status/Metadata/Decode/Limit → Answered.
   Is `is_connect()` a sound "no request byte was written" signal for reqwest 0.13 / hyper (pooled
   connections, HTTP/2, proxies, TLS handshake, connect timeout)? Are there paths where a request
   may have reached the server but `delivery()` says `NotSent`, or a response arrived but it says
   `Unknown`? Tests: checks/core-concepts/tests/w2_delivery.rs.
2. `concord::field::{single, unsigned}` (crates/concord-core/src/field.rs) and `ResponseHead`
   delegation: semantics identical to the previous `ResponseHead` methods?
3. `send P -> Template;` (crates/concord-macros syntax/model/emit): `_` replaced by each endpoint's
   output (`with_output`); declared result used for `send()` and `Sends<O>`; mismatch reported at the
   template. Hygiene, spans, `send value;` policies, stream/drain endpoints, cfg-gated endpoints,
   a template containing `_` in nested positions, a policy type that is not a plain path, macro
   wrappers forwarding the template. Tests: checks/core-concepts/tests/w2_declared_send_result.rs,
   crates/concord/tests/ui/send_declared_result_mismatch.*.
4. Generated endpoint docs naming a `query(..)` value's type; AUTHOR_GUIDE §5.11, §5.11b, §5.13 and
   CHEATSHEET rows: do the doc claims match the code?

Output: findings most severe first (file:line, defect, concrete failure scenario, CONFIRMED by
reading / PLAUSIBLE, smallest fix), then claims found sound, then areas not covered.
