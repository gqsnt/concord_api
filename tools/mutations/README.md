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

`requires_sockets` is an optional boolean. `checks` adds checks to the default
package suite. Use the values accepted by `mutate.py`.

| Check | Test targets |
| --- | --- |
| `macro-growth` | Name growth unit tests of `concord-macros`. |
| `macro-unit` | Unit tests of `concord-macros`, excluding the saved expansion comparison. A changed expansion alone does not establish a contract failure. |
| `macro-unused-calls` | Checks attribute placement and warning text on generated factory and setter declarations. |
| `macro-compiler` | The `compiler`, `call_rules`, and `call_surface` integration tests of `concord-macros`, including compiler fixtures and rendered signatures. |
| `generated-tour` | Library and integration tests of `tour-gen`. |

Never change a test's expected result to catch a mutation. A missing or ambiguous
`find` is stale and fails the check rather than counting as caught.

Run the new ID first, then the full catalogue. Inspect
`target/mutants/logs/<id>/` to confirm the failure checks the intended behavior.
`--timeout` bounds an ordinary test binary; `--build-timeout` bounds compilation
and compiler-test binaries. Logs and selections remain in
`target/mutants/last-run.txt` and `target/mutants/last-run.json`.

Run runner regressions with `python3 -B -m unittest discover -s tools/mutations`.

## Core body mutation contracts

The catalogue entries for repeatable request bodies link to the headings below. Each
states the clause in the form a mutation needs; the full text of each clause is in
`docs/reference/contracts.md`. The Rust tests carry the clause numbers.

### C12.12

A `Body::repeatable` is opened once for each attempt, and each attempt sends it
from its first byte. Nothing is opened when a call is built, cloned, inspected
or dropped. Whether a request may be sent again is still decided by the rules
of retry (the method, `idempotent`, `not_applied_on`): a repeatable body makes it
possible, not allowed. A multipart body with repeatable parts is repeatable.
Composition cooperates with the executor instead of opening an unbounded number
of immediately empty sources in one poll. Public `Body::map_stream` runs a fresh
map per opening: it may validate, frame or transform, but its output bytes must
be the same for the same input bytes, independent of attempts and input chunking.
A map cannot hide an observed factory-containing source failure. Mapping buffered
bytes still makes them one-shot.

### C12.13

A failure of the body is the attempt's failure. An error while opening or reading
ends the attempt as Other with the error as its source, whatever the transport
reports; a source that gives more or less than the declared length fails; after
its first error the body ends and opens nothing more.

### C12.14

An edit runs once and sees a placeholder that cannot be read. It must leave the
body alone: detected removal or replacement by a different kind is a Build error;
wrappers and look-alikes cannot always be detected, and the declared repeatable
body is sent regardless. Middleware and a signer see each attempt's own stream.
`request()` succeeds without opening anything. Two pages with the same head and
the same sources are the same request. A dropped call opens nothing more; the
timeouts cover opening and reading at the transport.
