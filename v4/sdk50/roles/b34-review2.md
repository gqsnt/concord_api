# Independent review: two small coordinator changes (between waves 3 and 4)

Read-only review; do not build or run tests; no external service.

1. `git -C /home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/b34-core show 304ed318` —
   every generated endpoint handle gains an inherent `pub fn operation() -> concord::Operation`;
   `Endpoint<T>::operation()` delegates to it; Square's `create_outcome_unknown()` uses it;
   test `checks/core-concepts/tests/b34_operation.rs`; AUTHOR_GUIDE §5.12, CHEATSHEET. Check: can
   the inherent method collide with any other generated or user-declared item (values, setters,
   scopes, endpoints named `operation`, `cfg`-gated endpoints, macro-forwarded declarations), or
   change method resolution for existing callers (`E::operation()` with `E: Endpoint<T>`, method
   call syntax on a value)? Is the identity identical in every case?
2. `git -C /home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/integration show d2f0b542` —
   two refused-connection tests get a 30 s client timeout instead of 2 s because Windows retries
   a refused connection for about two seconds. Is the explanation right, are the assertions
   unchanged, and can the tests now hang or flake on any platform?

Report only real defects: file:line, defect, concrete failure scenario, CONFIRMED / PLAUSIBLE,
smallest fix; then claims found sound; then areas not covered.
