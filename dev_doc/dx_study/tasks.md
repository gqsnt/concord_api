# Participant tasks

Use the supplied fixture and write down the answer before checking the key.

1. For `x?: Option<T>`, state the generated stored type and exact setter type.
2. Construct both “present with Rust `None`” and “Concord-absent”; explain
   `set_x(None)` versus `omit_x()`.
3. In both variants, perform a common present-value assignment and explain
   whether `with_x(T)` adds clarity or creates ambiguity. Preference alone does
   not earn credit; record correctness, time, corrections, and method search.
4. Place one scalar collection and one repeated collection with `each`; predict
   their query/header cardinality.
5. Modify a shaping block using assignment, `when_present`, `append`, `remove`,
   and `replace`; state what survives and when absence is a no-op.
6. Locate the effective request and response representations for three leaves
   and identify client, scope, local override, or fence provenance.
7. Add one endpoint below a nested representation override and one below an
   explicit fence; make the valid endpoint compile and diagnose the fenced one.
8. Review a semantic diff caused by a parent representation edit and list the
   affected capability, media, and acquisition changes.
9. Choose `Prefix` or `CompleteWithin` for a preview and for an all-or-error
   synchronization job; explain the consequence of reaching the bound.
10. Diagnose a supplied generated compiler error and identify the declaration
    line and valid generated method without editing generated output.
11. Explain acquisition, polling, terminal error, cancellation, and early drop
    for `ByteStream` versus `ItemStream<T>`.
12. State what an operation observer, debug output, rate feedback, pagination
    controller, response representation, and explicit `ApiResponse::headers()`
    may learn or change.

After each task, record confidence from 1 (guessing) to 5 (certain) before the
facilitator reveals correctness.
