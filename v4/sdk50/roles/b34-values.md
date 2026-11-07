# Between waves 3 and 4 — experiment: reading declared values back (policies and SDK code)

Working directory: a git worktree of the Concord workspace on branch `sdk50/b34-values`, from the
integration head (Wave 3 SDKs included). Build with `CARGO_BUILD_JOBS=4` and the worktree's
default `target/`, one cargo command at a time. You cannot commit; leave the work in the working
tree. Read nothing outside the worktree except the cargo registry. No external service.

This is an **experiment**: a compiled comparison that decides whether and how Concord changes.
Concord's product is its core, macros and generated usage; the hierarchy client → scope →
endpoint is fixed. Read `docs/AUTHOR_GUIDE.md` (§4 values, §5 policies) and
`crates/concord-macros/src/{syntax,model,emit}.rs` first.

## The evidence

Policies (`impl<T> SendWith<T> for P`, generic over every endpoint) and SDK code re-derive values
the declaration already holds:
1. `examples/quickbooks-online-sdk/src/policy.rs`: the create's `requestid` is parsed back from
   the built URL (the author's FRICTION F2; the maintainer's MAINTENANCE: a later write can
   silently omit it).
2. `examples/paypal-sdk/src/policy.rs`: the `PayPal-Request-Id` is read back from the native
   header, and which operations may be repeated is an allowlist apart from the declaration (the
   maintainer removed one entry: it compiled; only a new test caught it).
3. `examples/adyen-checkout-sdk/src/policy.rs` (`Confirm`): the payment reference is re-read
   from the URL path to check the provider's echo.
4. `examples/square-payments-sdk/src/error.rs`: an operation is compared through
   `<SquarePaymentsCreate as Endpoint<PaymentResponse>>::operation()` — the output type must be
   named to reach the endpoint's identity.
5. `examples/xero-accounting-sdk/src/lib.rs` (`ListInvoices`, `PageCursor`): values the generated
   handle already holds are mirrored because generated handles have setters but no getters
   ("this traversal's filter" in nine places).
6. Earlier: `examples/intercom-sdk` (page factory edits `op.search` through a private field) and
   `examples/square-payments-sdk` pages (clone the generated template, replace the cursor).

## Task

Prototype, on this branch, at least these two representations, each the smallest that works:

- **A — generated read access on handles**: getters for declared values on endpoint (and scope)
  handles, opt-in or by rule (never for `#[sensitive]` values; say which and why).
- **B — declared facts a generic policy can read**: e.g. SDK-chosen tags on endpoints or values
  that a policy queries through the `Endpoint`/`Prepared`/`Operation` it already receives (for a
  request id, "replayable under a key", a path reference), plus a way to name an endpoint's
  operation without its output type.

Port the witnesses above (at least QuickBooks, PayPal, Adyen, Square and Xero) to each
representation in the working tree, and measure for each: SDK production lines removed/added,
generated public items per endpoint (API growth), compile-time guarantees gained (e.g. a write
that omits its request id no longer compiles), and any counterexample where the representation
does not fit. A representation that does not subtract is a valid result. Do not edit or remove
any existing assertion; all touched packages' tests, clippy (`-D warnings`), rustdoc
(`-D warnings`) and `cargo fmt --all -- --check` must pass for whichever variant you leave in the
tree (leave the better one; keep the other as a patch file under `target/tmp/` and describe it).

Report: each representation (syntax, generated code, signatures), the per-SDK measurements, the
counterexamples, which you recommend and why, and what you could not decide.
