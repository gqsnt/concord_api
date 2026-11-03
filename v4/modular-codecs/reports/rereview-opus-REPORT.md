# Re-review of the corrections: Concord V4 modular codecs, 9f8939f → 530b6c1 (side Opus, frozen)

All twelve earlier findings (S1–S4, M1–M8) are closed. The corrections add three minor problems, listed under "New findings". I found nothing blocking. This report makes no acceptance claim.

**Inputs**
- Corrected snapshot: `snapshots/rereview-opus`. `git status` stayed clean, as did `final-sem-opus`.
- The diff patch.
- REVIEW_TRIAGE, section "Final semantic review".
- DECISIONS MC14 and MC16–MC19, and the TEST_MATRIX changes.

**Not read:** `final-exec-sol`, `rereview-sol`, `codex`, and `v4/modular-codecs/reports/final-exec-sol-REPORT.md` inside the snapshot.

**Where I worked:** `/home/canarit/projects/concord_v4_modular_work/proposals/final-sem-opus/`, in `probe-rr`, `probe-seal-rr` and `mutant-s1`. Build output went to `target-rr` and `target-mut`, using `cargo +1.99.1 --offline`.

## Earlier findings

### S1 — closed
- `ui/native_key_sealed.rs` now imports `concord::__macro::concord_core::__private::NativeKey`. Its `.stderr` shows E0277 on `__private::sealed::Sealed`.
- **Pristine copy (`mutant-s1`):** I ran the real UI suite (`cargo test -p concord --test ui`). All 77 cases pass, `native_key_sealed` included.
- **Mutant:** I removed `: sealed::Sealed` at `__private.rs:737` in the same copy. Exactly 1 of 77 fails: `native_key_sealed.rs` reports "Expected test case to fail to compile, but it succeeded". The test now guards the seal.
- `probe-seal-rr` against the snapshot still fails with E0277 on `Sealed`.

### S2 — closed
The overclaim is narrowed in:
- `names.rs:9-22` (core rustdoc);
- the generated rustdoc (`emit.rs:2008-2017`);
- AUTHOR_GUIDE §4.9, lines 556-567;
- MC14 and the test header comment.

A new test, `validate_names.rs::earlier_build_steps_fail_first`, pins the order. Rerunning P1 gives the same behaviour as before:
- the path `Display` failure is reported first;
- the encoder failure is reported first;
- the plain endpoint reports the header error.

That now matches the documentation: the problem is reported "unless an earlier step fails first", and no request of a reported endpoint can be built.

Residual wording (minor):
- AUTHOR_GUIDE l.594 still says "(the error that endpoint's `prepare()` reports)" with no qualifier. It sits right after the corrected paragraph.
- The hidden doc of `NameCheck::endpoint` (`__private.rs:998`) says "the one its `prepare()` would report".
- REVIEW_TRIAGE row R2 says "each problem equals `prepare()`'s error".
- The comment at `validate_names.rs:115` says the same; it is accurate for that fixture only.

### MC19: I accept the outcome but disagree with the rationale
MC19 keeps the build order of `prepare()`. I accept that, since the docs are now accurate, but its reasons are weak:
- **"It would change which error a request reports."** That is true only for requests of endpoints that can never be built. For those, a value-free check placed first would give a deterministic error that does not depend on the values. Kept last, the error varies with the values (P1).
- **The encoder still runs before the failure.** It runs, consuming a one-shot payload, before an unconditional failure.
- **SEMANTICS §6.** Its rule "later values are not formatted" would still hold if the value-free check ran first; only one sentence would change.
- **"The validator runs once, before any request."** Nothing makes users call it.

This is a debuggability cost, not a correctness one. I record it as a note, not as an open finding.

### S3 — closed
The wording is fixed in:
- `codec.rs:71-75`;
- CUSTOM_FORMATS 139-143;
- AUTHOR_GUIDE 929-932;
- MIGRATION 233-234.

All of them now say that a refusal in `decode_body` is a `Decode` error after the body was read. Rerunning P2 gives the documented behaviour: `Decode`, `response()` is `None`, and Display shows "(version 3)".

### S4 — closed
- MIGRATION l.106 now reads "(or your client's default `Accept`)".
- MIGRATION 111-115 covers a client-wide default `Accept` being replaced, with remedies.
- AUTHOR_GUIDE §5.8 states the same.

### M1 — closed
- `error.rs:11-16` now says positions are numbers the format chose, possibly read from a header, and never values "as text". It points to the facade's documentation for the exact scope.
- CALLER_GUIDE §6 is aligned.

### M2 — closed
The `Decode::context` docs (`codec.rs:156-158`) say: "may read `Content-Type` ... should not refuse on it ... fall back to a default".

### M3 — closed
RECIPES lines 20-31 now describe the `status()` guard that `examples/native/src/errors.rs` already has.

### M4 — closed
- `mime` now parses only the text before the first `;`. A new RFC 9110 parameter scanner (`media.rs:44-130`) reads the parameters.
- P6 rerun: all seven forms are accepted, including the two it refused before (whitespace before `;`, and `;;`).
- **Disagreement hunt** (`probe-rr/tests/parser.rs`): about 55 inputs, compared against `mime` parsing the whole string.
  - The scanner is right where `mime` was wrong:
    - quoted pairs: `charset="\u\t\f\-\8"` reads as `utf-8` (`mime` kept the backslashes);
    - an escaped quote: `x="a\"; charset=latin1"` is accepted, with no charset (`mime` refused it);
    - a `;` inside a quoted value: `x="a;charset=latin1"` adds no charset;
    - a tab before a parameter is accepted;
    - an unterminated quote, `charset="utf-8\"`, is refused (`mime` read it as `utf-8\`).
  - The only input `mime` accepted that is now refused is `x=` (an empty unquoted value). The grammar does not allow it, so the refusal is correct.
  - Comma-joined lists, stray quotes, `=` with nothing before it, `==`, a parameter without `=`, and unquoted `/`, `:`, `@` are all refused. These are correct.
  - It cannot panic: the text is visible ASCII plus tab after `to_str`, so byte slicing is safe.
  - Essence quirks that remain (note only):
    - type and subtype still follow `mime`'s grammar (it accepts `~` and `%`);
    - leading whitespace is refused. That can only happen through a hand-built `ResponseHead::new`, since hyper trims field values.
  - This is a second, hand-written parser despite the handoff's "do not invent a second parser". It is narrow and justified by the `mime` 0.3.17 bugs, but MC17 should say so explicitly.

### M5 — closed
Documented in MIGRATION l.116-118 and AUTHOR_GUIDE §5.8. Rerunning P5 still gives two `Accept` lines, which is now the documented behaviour.

### M6 — closed
Each item checked:
- the macros `lib.rs` doc;
- MAINTAINERS;
- the core `lib.rs` and README dependency lists;
- the facade's `extras` feature bullet;
- `concord::CODEC_ERROR_CONTEXT` is re-exported; it compiles in `probe-rr`.

### M7 — closed
The stray file `A` is gone.

### M8 — closed
`crates/concord-macros/README.md` exists, with a `readme` field. `cargo package --list -p concord-macros` on the pristine copy includes `README.md`. The README states the support boundary: depending on the macro crate directly is not supported.

## New findings

**N1 (minor): docs were not updated for the repeated-`charset` refusal (X-F2/MC17).**
These still describe the old rule:
- AUTHOR_GUIDE §5.1 l.696, for `Text`: "only a declared non-UTF-8 `charset` is refused";
- MIGRATION l.82-83: "`Text` refuses only a declared non-UTF-8 `charset`";
- SEMANTICS §7 l.260-261, the "precise rules" document, for both Json and Text;
- the `concord-extras/src/lib.rs:7` table.

Reproducer: `Text` on `text/plain; charset=utf-8; charset=UTF-8` returns `Err("the response Content-Type declares charset more than once")`. CUSTOM_FORMATS and the Text rustdoc were updated.

**N2 (minor): `Text`'s new refusal contradicts its own leniency.**
`Text` now refuses a repeated but equal UTF-8 `charset` parameter. It still accepts more ambiguous inputs:
- two `Content-Type` lines, one of which says `charset=iso-8859-1`;
- `text/plain; charset=latin1, text/html`;
- `text/plain; profile=http://x; charset=latin1`, where an unrelated malformed parameter hides a declared latin1 charset.

All of these were probed. `Text`'s stated policy is "a decoder, not a media type; missing, repeated or malformed accepted", so refusing the unambiguous case while accepting the conflicting ones is backwards. Either refuse only when a readable charset is not UTF-8, or document the asymmetry. Json and Form refusing repeats is consistent with how strict they are.

**N3 (minor): a name clash in CUSTOM_FORMATS §5 (new paragraph, after the `api!` excerpt).**
The paragraph suggests an output wrapper `Versioned<T> { version, value }`, declared as `-> Versioned<Item>`. In the same section `Versioned` is already the format type, used as `response Versioned;`. The two cannot coexist in one module, and the text reads ambiguously. Rename the wrapper (for example `WithVersion<T>`). The paragraph also separates the excerpt from "The same test sends…", which refers back to that excerpt.

## Notes
- The strict `charset` policy (refuse `utf8` and any non-UTF-8 value) is kept deliberately, with `IgnoreContentType` as the escape. That is acceptable.
- The new `HeaderNameErrors` `Debug` prints only the static problem lines. It leaks nothing.

## What I executed and what I reasoned

**Executed:**
- P1–P6 against `rereview-opus` (`probe-rr`, 7 tests, all passing; outputs quoted above);
- `probe-seal-rr` (E0277);
- the new `tests/parser.rs` battery: 3 tests, about 55 inputs for Json, Text and Form, plus a comparison with whole-string `mime`;
- the real UI suite on a pristine copy and on the unsealed mutant;
- `cargo package --list -p concord-macros`;
- the `CODEC_ERROR_CONTEXT` re-export, compiled in `probe-rr`;
- `git status` on both snapshots.

**Reasoned (not run):**
- every documentation check and line reference;
- the judgement of MC19;
- the `Text` policy argument (N2), though the inputs it cites were run.

**Not run:** the candidate's full gate, Windows, and the other features and packages.
