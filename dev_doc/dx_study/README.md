# Concord v2.1 unfamiliar-developer comprehension study

This package is ready for a facilitator who is not the primary implementer.
Recruit at least three Rust developers who have never used Concord; five is
preferred. Do not coach participants on Concord semantics. Record anonymized
participant IDs only and obtain consent under the facilitator's applicable
policy before collecting screen, audio, or telemetry data.

## Run protocol

1. Check out the exact candidate commit and record it in `results.md`.
2. Give each participant a fresh copy of one counterbalanced variant from
   `variants.md` and the task sheet from `tasks.md`.
3. Permit normal compiler, Rustdoc, repository search, and editor use. Do not
   answer semantic questions during a timed task.
4. Record correctness, elapsed time, files/doc pages opened, compiler cycles,
   corrections, misconceptions, confidence, and confidence calibration using
   `rubric.md`.
5. Rotate variant order. Do not let one participant see another's work.
6. An independent reviewer scores the artifacts, then appends anonymized rows
   to `results.md`.
7. Apply the stop/decision rule in `decision_record.md`. Commit raw anonymized
   worksheets only when consent and repository policy permit; never commit
   names, emails, recordings, or screen captures.

The primary implementer may facilitate logistics but cannot be the sole
participant or sole scorer. AI cold reads may be appended as supplementary
evidence and never count toward the human minimum.
