# Required body arity: native diagnostic capture

This driver closes the missing/extra required-body argument portion of V3-L05.
It adds no active UI fixture until a reviewer integrates the compiler-produced
snapshots. The default product candidate is `579e8eac9a436a57ebc5ea3fcc2e7da3cf330c31`;
the driver checkout has its own separately recorded identity.

## Capture and acceptance

The workflow installs Rust 1.97.1 and creates an exact detached temporary
worktree. It copies the two inert fixture sources into
`crates/concord/tests/ui/fail/capabilities/` and appends one temporary test,
`required_body_arity_capture`, to the isolated harness. That test reuses
`tests/ui/pass/language/current_language.rs` as its maintained positive control
and compiles the two arity failures.

The first native invocation uses `TRYBUILD=overwrite`; no expected stderr text
is supplied. Its recorder returns a non-stable-input verdict because it
generates the two snapshots. The driver preserves that receipt and permits
exactly those two file additions, requiring a successful native exit and no
timeout or recording error. Each snapshot must contain exactly one E0061
diagnostic. The second invocation removes overwrite and must pass with stable
inputs. A timeout or incomplete receipt remains a failure to qualify.

The artifact contains both journals and snapshots, source and driver identities,
environment/toolchain data, the two native stderr files, fixture source hashes
and a proposed complete final `ui.rs`. Build caches are not uploaded.

## Integration after review

Copy the four files named `required_body_argument_{missing,extra}.{rs,stderr}`
from artifact `integration/` into the capabilities fixture directory. Add their
two paths immediately after `absent_value_controls.rs` in the explicit list
inside `current_language_surface_and_diagnostics`. The artifact's `ui.rs`
contains this exact final harness, without the temporary capture test.

Then run the maintained UI matrix on the integrated commit **without**
`TRYBUILD=overwrite`. The capture worktree success establishes actual compiler
output; the integrated matrix establishes final inclusion. No product file or
branch is mutated by the capture script.

The script and workflow were reviewed remotely; native execution is the purpose
of this capture job, not a result claimed by the draft.
