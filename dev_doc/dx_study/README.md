# Gate 3 unfamiliar-human final-surface comprehension packet

This packet is ready for a facilitator who is not scoring their own Concord
implementation work. It does not compare syntax variants: `with_x` is absent
from v2.1, and reopening it requires a separate paired, counterbalanced study
with four to six qualifying participants.

## Qualifying participants

Recruit at least three consenting humans who can read ordinary Rust and
generics, have not implemented or deeply reviewed Concord, have not read the
complete v2.1 design study, and plausibly represent endpoint-author, caller, or
reviewer roles. Record only an anonymized ID and a coarse Rust background band.
The primary implementer, maintainers trained by the design process, AI agents,
and generated personas do not count.

One participant may complete the independent first-contact Rustdoc review
before receiving this packet. A participant's later trained understanding
cannot be reused as Gate 10 first-contact evidence.

## Run order

1. Check out one exact candidate and record its full SHA and toolchain in
   `results.md`.
2. Obtain consent for anonymized timing and notes. Do not commit names, contact
   details, audio, video, or screen recordings.
3. Give the participant `tasks.md`, public/generated documentation, a normal
   editor, and compiler access. Do not give them this README, `rubric.md`, or an
   answer key during scored work.
4. Do not coach during a scored task. Record the participant's initial answer,
   correctness, elapsed seconds, confidence, documentation/pages opened,
   searches, compiler cycles, wrong model/terminology, recovery, and
   abandonment.
5. Have an independent scorer apply `rubric.md`. Append anonymized per-task raw
   rows and aggregates to `results.md`.
6. Classify findings as Critical, Major, or Minor. Fix every Critical and every
   release-relevant Major finding, then rerun the affected tasks with qualifying
   humans before passing Gate 3.

The empty results record is evidence that the gate has not run, not a waiver.
