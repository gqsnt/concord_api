# Gate 3 scoring and decision rubric

## Per-task score

- `2`: correct result and correct ownership/reasoning without coaching;
- `1`: correct only after compiler/public-documentation feedback or with a
  materially incomplete explanation;
- `0`: incorrect, abandoned, implementation source required, or a
  facilitator-supplied answer.

Record one row per participant and task with:

| Field | Value |
|---|---|
| anonymized participant | `P1`, `P2`, or `P3` (and higher if recruited) |
| role/background band | endpoint author, caller, or reviewer; coarse Rust experience |
| score/correctness | 0–2 plus expected/observed answer |
| elapsed | seconds |
| confidence | 1–5 before feedback |
| documentation/context | exact public pages/types opened |
| search/compiler behavior | queries and compiler cycles |
| wrong model/terminology | concise code from the list below or new code |
| recovery | none, docs, compiler, or abandoned |
| severity | Critical, Major, Minor, or none |

Suggested misconception codes: `presence-option`, `set-omit-reset`,
`collection-cardinality`, `patch-operation`, `representation-provenance`,
`directionality`, `explicit-fence`, `effect-retry`,
`pagination-completeness`, `stream-lifecycle`, `generated-error`,
`observer-authority`, and `semantic-diff`.

## Severity

- **Critical:** causes wrong wire, security, retry, completeness, or lifecycle
  behavior.
- **Major:** blocks a normal task or requires implementation reading.
- **Minor:** naming/search friction without semantic error.

## Pass rule

Gate 3 passes only if all of these are observed with at least three qualifying
humans:

- no Critical misconception is shared by two or more participants;
- every Critical task is completed correctly by at least two of three
  participants without coaching;
- aggregate scored correctness is at least 80% of available points;
- confidence is not systematically high (4–5) on wrong answers;
- normal tasks do not require macro/core implementation source;
- every Critical and release-relevant Major finding is fixed and the affected
  task is retested successfully.

Report individual results, medians, ranges, confidence calibration, and
abandonments. Do not replace failure analysis with an average.
