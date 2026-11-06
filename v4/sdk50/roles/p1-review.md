# Independent review: SDK50 P1 core and macro changes (read-only)

You review a candidate change to the Concord Rust library. Your working directory is the
candidate checkout at the branch `sdk50/p1-draft`; the base is `5c3ef5f6`. Read `AGENTS.md`/
`CLAUDE.md` if present, then `git log --oneline 5c3ef5f6..HEAD` and `git diff 5c3ef5f6..HEAD`
for `crates/` and `checks/core-concepts/`. You cannot modify files or build; your final message is
the review. You did not write this code.

## The claims to falsify

F2 (response interpretation, `crates/concord-core`):
- Every error raised after a response head arrived (`Status`, `Metadata`, `Decode`, `Limit`,
  mid-body `Transport`, and a failed drain) carries that head; none before a head. The head is
  moved (`std::mem::take` of the header map) before the body is read and nothing reads the
  native header map afterwards (reqwest decompression, `bytes()`, `chunk()`).
- `Received::decode_with_meta`/`decode_limited_with_meta` return the same value and errors as
  `decode`/`decode_limited`, plus the head; stream outputs and `Incoming` keep their response so
  their head is a copy; `decode_within` (the old path) is unchanged in behavior.
- `Received::document::<F, D>(limit)` runs `F::check_content_type` and `F::context` before polling
  the body (a refusal never waits for a stalled body), reads bounded, then `F::decode_body` with
  that context; any status; returns the head beside the outcome; a `Decode` failure keeps the
  body bytes (`Error::body_unredacted`), and nothing new is printed by `Display`/`Debug`.
- `Error::meta()` now returns `Some` for more kinds. Is that a compatibility problem for any
  caller pattern you can construct? `Error::status()` is unchanged.

F3 (declarations, `crates/concord-macros`):
- `params Name(..);` on the client, `..Name` in an endpoint: values spliced at the written
  position (wire order), body position adjusted, spans of the group kept; the generated trait
  `Name` forwards to the endpoint's inherent setters (`Self::set_x`), implemented only for
  including endpoints, with their `cfg`s. Look for: hygiene (a user type or trait named like the
  group, a group named like a generated type, `r#` identifiers, a value named `self`), visibility
  (private value types in a `pub` client), `#[cfg]` endpoints, `#[deprecated]` endpoints,
  inherited `else inherit`/`remove` interplay, generated docs/rustdoc warnings (`missing_docs`
  on the trait or its methods), and whether the trait methods' `impl IntoValue<T>` arguments
  match the inherent setters exactly.
- The diagnostics cover: required value in a group, unknown group (near-name suggestion), group
  on a scope, group declared below the client, unused group, value on both endpoint and group.

## Output

Findings first, ordered by severity, each with: file:line, the smallest concrete counterexample
(input + expected vs actual), and a fix. Then: simpler alternatives you would have preferred for
F2 or F3 and why (compare complete SDK/caller code, not snippets), and what evidence would make
you reject either change. Mark anything you could not verify without running code as
UNVERIFIED. "No findings" is a valid answer only if you looked for each listed risk.
