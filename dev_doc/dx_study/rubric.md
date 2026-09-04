# Scoring rubric

Score each task 0–2:

- 2: correct result and correct ownership/reasoning without material prompt;
- 1: correct after one self-correction or with an incomplete explanation;
- 0: incorrect, abandoned, or facilitator-supplied answer.

Record these measures per task:

| Field | Unit |
|---|---|
| correctness | 0, 1, or 2 |
| elapsed | seconds |
| files/docs opened | count and paths |
| compiler cycles | count |
| corrections | count |
| misconception codes | list |
| confidence | 1–5 before reveal |
| calibration error | absolute difference between confidence-normalized score and correctness-normalized score |

Misconception codes: `presence-option`, `set-omit`, `with-ambiguity`,
`collection-cardinality`, `patch-operation`, `representation-provenance`,
`fence`, `semantic-diff`, `pagination-completeness`, `generated-error`,
`stream-lifecycle`, and `observer-authority`.

Minimum comprehension threshold: every participant scores at least 18/24, at
least 80% of participants score task 2 and tasks 6–12 correctly, and no
high-confidence (4–5) misconception repeats across two participants. Report
medians and ranges; do not replace individual failure analysis with averages.
