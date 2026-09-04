# Gate 3 decision record

## Selected v2.1 surface

ADR 0012 selects `set_x(Option<T>)`, `omit_x()`, and initialized-only
`reset_x()`. The generated `with_x` alias and its namespace reservation are
absent. The final-surface comprehension protocol evaluates that exact surface;
it does not postpone a v2.1 syntax decision.

## Current evidence state

Zero qualifying humans have completed the protocol, so comprehension remains
Unknown and Gate 3 is Blocked. The repository must remain a
`2.1.0-beta.N` integration candidate and cannot merge under the strict policy.

## Stop rule

Only the thresholds in `rubric.md`, followed by fixes and successful retests of
all Critical and release-relevant Major findings, can move Gate 3 to Passed. AI
review, implementer familiarity, compiler tests, and Rustdoc generation do not
substitute for this evidence.
