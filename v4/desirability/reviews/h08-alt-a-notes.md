# H08 alternative A — coordinator notes (not a review)

Implementer: Claude subagent (Opus 5.5); committed `fd06686f` on local `dz/h08a` (from `ec643a54`).
Boundary audit: 0 tool calls naming `work/sealed`, `work/baseline` or `tools/desirability`.
Result (implementer's evidence in `work/dev/h08a/`): an endpoint declared without a class fails the
Strava SDK build (E0277 with an SDK `on_unimplemented` message), but the first diagnostic names the
output type (`Activity`, shared by two endpoints) and points at the client's `send` statement, not
the endpoint — the card's naming condition is not met by A. Public view: 55 items added, 33
signatures changed (`prepare()` returns `Prepared<Classed<C, X>>`; `Endpoint<X>` becomes
`Endpoint<Classed<C, X>>`); generated docs show the projection type. +157/−56 lines; Strava tests
63/63 unchanged. One hand-written comparison left (`error.rs` uncertain-wording). Retry delay for
write classes 25 ms → 0 ms (unobservable: one attempt).
