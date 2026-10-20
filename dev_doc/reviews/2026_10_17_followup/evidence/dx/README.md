# V3 AI-agent exercise evidence

This bundle supports [the exercise report](../../dx_exercises.md). It describes
one AI agent and native commands; it contains no human-participant results.

| Artifact | Meaning |
|---|---|
| [initial-predictions.md](initial-predictions.md) | Pre-feedback predictions/confidence and later fixture-selection notes, with mistakes retained. |
| [checked-answers.txt](checked-answers.txt) | Public-doc answers, corrections and subsequent actual rustdoc/CLI verification. |
| [catalog-effect.patch](catalog-effect.patch) | Exact isolated source edit against `570eec460dee1d83828ae48e35396ae07652aabc`. |
| [catalog-before.json](catalog-before.json) | Native export of the original Catalog declaration. |
| [catalog-after.json](catalog-after.json) | Native export after the one-line authored effect change. |
| [catalog-effect-diff.json](catalog-effect-diff.json) | Original CLI JSON extracted verbatim from its retained log. |
| [manifest.json](manifest.json) | Artifact digests and portable command, original-command, receipt, source-state and log evidence. |

The checked-answer record is retained as plain text so its original illustrative
fragments remain evidence rather than normative documentation examples. Renaming
the artifact does not change its bytes or digest.

The initial/checked working records normalize local path prefixes for
portability; their original digests are included. The manifest's portable
commands replace original absolute output paths with the adjacent filenames.
Their original argument arrays are separately pinned by digest, as are the
unaltered native receipts/logs retained in the task workspace. A dirty
follow-up source state is recorded as dirty. The contract before/after commands
ran in their own original-candidate worktree.

All duration fields measure native processes, including compilation and Cargo
lock waiting. They are not participant reading or completion times. Passing
the eight executable exercises is behavioral evidence, not a reconstructed
human study.
