# Independent review: the fix for the b23-review4 finding

Read-only review. The working directory is a git worktree of the Concord workspace on
`sdk50/integration` at `435ad897`. Review `git show 435ad897` (one commit): `with_output()` in
crates/concord-macros/src/emit.rs and its tests. Do not build or run tests; read code. No external
service. The finding it fixes is in
`/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/roles/b23-review4-sol/last.md`.

The claim: a type macro's input is parsed with each non-lifetime `_` renamed to a numbered marker
ident, only to find which `_` are type placeholders (a single-ident type path naming a marker;
expressions skipped; nested macro inputs by the same rule); then those `_` are replaced in the
original tokens and every other token is kept as written, spacing included; groups are rebuilt
only when they changed. Check: can the renaming change how the input parses (a `_` that is valid
where an identifier is not, or vice versa), so that a placeholder is missed or a non-type `_` is
filled? Is the marker numbering identical in both walks? Are lifetimes, invisible groups and
spans preserved? Any regression for the earlier cases (`fn(_: u8) -> _`, `[_; { let _ = 1; 2 }]`,
`result!{_}`, forwarded fragments)?

Report only real defects. Output: findings most severe first (file:line, defect, concrete failure
scenario, CONFIRMED by reading / PLAUSIBLE, smallest fix), then claims found sound, then areas not
covered.
