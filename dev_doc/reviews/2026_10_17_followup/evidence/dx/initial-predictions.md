Portable copy of the AI-agent working record. Only local path prefixes
and an accidental prose separator were normalized; initial predictions,
mistakes, confidence and later corrections remain visible.

# V3 exercise initial predictions — AI agent

Candidate: `570eec460dee1d83828ae48e35396ae07652aabc`, branch `concord-v3`.
Participant: the `/root/v3_dx_exercises` AI agent; this is not an independent human study.
First content read: `dev_doc/dx_study/tasks.md`. Before this record, only the packet,
file names, possible AGENTS.md files (none found in ancestors), revision, status,
and tool availability were checked. No macro/core source or rubric was read.
Cargo and rustc are absent from PATH. No compiler execution is claimed.
Per-task elapsed time was not instrumented; no human reading or recovery time is inferred.

The packet does not supply the three endpoint declarations in task 5, the
examples in task 7, or the generated error in task 10. Public fixtures will be
selected and identified before solving those parts. These are reproducibility gaps.

## Initial answers, before checking public documentation

| Task | Prediction | Confidence (1–5) |
|---|---|---:|
| 1 | `x?: T` stores `Option<T>`; `set_x(T)`, `replace_x(Option<T>)`, `set_x_if_some(Option<T>)`. `x?: Option<T>` stores `Option<Option<T>>`; `set_x(Option<T>)`, `replace_x(Option<Option<T>>)`, `set_x_if_some(Option<Option<T>>)`. | 4 |
| 2 | Every listed call is valid for `T = i32`. `set_x(None)` stores present inner `None`; `set_x(Some(7))` stores present inner `Some(7)`; `replace_x(None)` makes the slot absent; `replace_x(Some(None))` stores present inner `None`; `set_x_if_some(None)` leaves the prior state alone; `omit_x()` makes the slot absent; `reset_x()` returns the initializer, or absence without one. | 4 |
| 3 | `each tags` emits one query occurrence per element without changing a declared collection type. Zero, one, and two elements mean zero, one, and two `tag` occurrences. | 4 |
| 4 | Ordinary assignment overwrites inherited values for the same key, including removal on optional absence. `when_present` overwrites only on presence and otherwise preserves inherited values. `append` adds occurrences when present and preserves inherited values on absence. `remove` clears a named key. I predict block-level `replace` clears all inherited patches before applying the local block, and `off` disables inherited query/header values entirely. Exact syntax and whether `off` blocks deeper declarations are uncertain. | 2 |
| 5 | Deferred until the missing three-endpoint fixture is selected. General prediction: request and response selection inherit independently from the nearest declaration owning that direction; an endpoint can inherit one and override the other. | 3 |
| 6 | An endpoint override changes only that endpoint; a scope override changes its descendants unless locally overridden. An `explicit` fence cuts inheritance and requires each endpoint or descendant scope to select the fenced direction. A missing direction should point to the endpoint and name request/response representation as missing. Exact syntax is unknown. | 3 |
| 7 | General prediction pending selected examples: endpoint status is an explicit set or default 2xx, rather than a policy decision. Safe calls may retry if their bodies can replay; idempotent calls may retry if their bodies can replay; non-idempotent calls should not policy-retry without an explicit safety capability. Any retry also needs a qualifying failure and budget. Exact defaults and body classification need checking. | 2 |
| 8 | `collect_prefix(n)` returns up to n pages/items successfully even when incomplete; `collect_all(nonzero)` errors if its bound is hit before completion; `pages(n)` leaves traversal in application control and stops at a bound. I am uncertain whether bounds count items or pages and whether `pages(0)` rejects or makes no requests. `collect_prefix(0)` probably returns empty; `collect_all` rejects zero through its argument type. | 2 |
| 9 | Acquisition produces a fallible byte/item stream handle after request/header processing; polling yields fallible byte chunks or typed items. Transport, framing, or decode error terminates the stream, with no transparent replay after delivery starts. Cancellation produces one terminal cancellation error; early drop stops consumption and releases resources without draining. The exact terminal error visibility is uncertain. | 3 |
| 10 | Deferred until the absent compiler-error fixture is selected. Likely diagnose an optional setter or ownership mistake at the DSL declaration, then call the matching generated method without touching generated output. | 1 |
| 11 | Observer and debug expose value-free metadata and redacted summaries; rate-limit callbacks receive only admission/routing information needed for policy, without secret values or broad body access. Explicit request plans and response handles may expose URLs and headers because their caller deliberately asks for them. Exact authority fields and redaction boundaries need checking. | 3 |
| 12 | Deferred until a concrete local public fixture and change are chosen. An endpoint-local representation/effect edit should report that endpoint only. Representation can alter caller and wire classification, while effect changes runtime retry semantics; security classification might change for request media types or credentials, but this is uncertain. | 2 |

Initial answers are preserved here; checked answers and corrections belong in a
separate results file. A result learned from later docs must not be retroactively
inserted into this table as an initial prediction.

## Concrete fixture selection and remaining predictions

Task 5 uses the three endpoints in the public `representation_inheritance_contract`
anchor at `crates/concord/tests/generated/representation_inheritance.rs` (also
embedded in `docs/spec.md` §9): root `inherited`, `overrides.local`, and
`fenced.explicit`. The initial general answer above predates looking at this
fixture; endpoint-specific answers are recorded as checked answers, not an
invented earlier prediction.

Task 7 uses maintained `docs_spec::outcomes::OutcomeApi` endpoints `properties`
(`METHOD("PROPFIND")`, explicit safe), `upsert` (`PUT`, explicit idempotent,
Json body), and `create` (`POST`, default non-idempotent, Json body). Their exact
answers were obtained after reading public retry/status guidance. The general
prediction above remains the only pre-feedback task-7 answer.

Task 10 selects `crates/concord/tests/ui/fail/capabilities/clear_optional_control_absent.stderr`.
After reading the supplied error but before its `.rs` author fixture, my
prediction is that the optional `category?: String` declaration generates
`omit_category()`, not `clear_category()`. The error represents a mistaken
single generic "clear" operation where Concord names absence explicitly.
Confidence: 5, informed by the public value-control table read on earlier tasks.

Task 12 will change only the canonical Catalog `regional.update_item` declaration
from `effect idempotent;` to `effect non_idempotent;`. Before running any export
or diff: expected affected endpoint is `catalog.item.update` only; caller
method/input/output and wire contract stay equal; runtime retry/recovery safety
changes. I predict a `RuntimePolicy` classification, possibly a provenance
entry because the authored effect changes, with no `CallerApi`, `Wire`, or
`AuthAuthority` entry. Confidence: 3. Actual export/diff requires a Rust toolchain
that the root agent is installing. No diff result is claimed yet.
