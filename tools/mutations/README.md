# Mutation checks

For contributors checking whether tests catch bugs: this catalogue explains the faults that `just mutants` tries.

*[Concord](../../README.md) · [contributor tools](../README.md) · mutations*

A mutation makes one intentional source fault. The suite must catch it by a test
failure, a compilation error, or a time limit. A surviving mutation shows a gap in
the checks. Run `just mutants --jobs 1`; select IDs to investigate one fault, for
example `just mutants --jobs 1 M01`.

Set `CONCORD_SKIP_SOCKETS=1` when local TCP sockets are unavailable. The shared
socket inventory excludes those tests and mutations marked `requires_sockets`.
The final line reports caught, survived, stale, and socket-only counts.

Each invocation copies the repository, including documentation, beneath
`target/mutants/`. It excludes Git metadata and build artifacts. Sources are
restored after each fault; working sources are never changed. A clean baseline
must compile and pass before mutations run. Cargo commands run sequentially.
Child processes retain the invocation lock so a terminated parent cannot allow
another invocation to delete active build files.

Add one JSON file with an unused `M` identifier and these fields:

| Field | Meaning |
| --- | --- |
| `id` | Identifier matching the filename, such as `M148`. |
| `area` | Behavior affected by the fault. |
| `contract` | Existing contract number(s), or a current page heading (`docs/page.md#heading`). |
| `description` | One sentence describing the caller-visible fault. |
| `file` | Source path relative to the repository root. |
| `find` | Exact original text, occurring once in that source file. |
| `replace` | Replacement text; may be empty. |

`requires_sockets` is an optional boolean. `checks` optionally adds public-surface,
documentation, or allocation checks; use the values accepted by `mutate.py`.
Never change a test's expected result to catch a mutation. A missing or ambiguous
`find` is stale and fails the check rather than counting as caught.

Run the new ID first, then the full catalogue. Inspect
`target/mutants/logs/<id>/` to confirm the failure checks the intended behavior.
`--timeout` bounds an ordinary test binary; `--build-timeout` bounds compilation
and compiler-test binaries. Logs and selections remain in
`target/mutants/last-run.txt` and `target/mutants/last-run.json`.

Run runner regressions with `python3 -B -m unittest discover -s tools/mutations`.
