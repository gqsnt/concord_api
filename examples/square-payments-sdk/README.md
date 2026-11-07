# Square Payments SDK33

`concord-sdk-square-payments` is a bounded, example-assisted Square sandbox
slice at API version **2026-09-16**. It creates an immediate-capture payment from
an already collected token, retrieves a payment, and pages account payments with
an opaque cursor. For delayed capture it creates an authorization-only payment
and completes or cancels it. Models retain ID, integer amount/currency, status
and version token. Other response fields are ignored. **Live provider calls: NOT_RUN.** All verification
uses `examples/testserver` on loopback.

Assistance came from documentation excerpts and the shared testserver; no existing
provider SDK source file was consulted.

Use the public generated hierarchy:
`Square → payments() → create/get/list/complete/cancel`.
`send()` returns `Result<T, Error>` without an extension trait or wrapper client.

```no_run
use concord_sdk_square_payments::{
    Square, SquareConfig, CreatePayment, Money, Error, Resume, bearer, transport,
};
use std::{num::NonZeroUsize, time::Duration};

// Pass a local TestServer URL and a synthetic token for fixture execution.
async fn workflows(base_url: &str, token: &str) -> Result<(), Box<dyn std::error::Error>> {
    let api = Square::new(transport(Duration::from_secs(10))?, SquareConfig {
        authorization: bearer(token)?,
    }).with_base_url(base_url)?;

    let body = CreatePayment::new("caller-chosen-unique-key", "collected-token", Money {
        amount: 1000, currency: "USD".into(),
    })?;
    let operation = api.payments().create(body);
    let retained = operation.clone();
    let created = match operation.send().await {
        Ok(response) => response.payment,
        Err(Error::Concord(error))
            if error.transport().is_some_and(|e| e.is_timeout()) => {
                // Explicit caller decision: one replay with the exact original key/body.
                retained.send().await?.payment
            }
        Err(error) => return Err(error.into()),
    };
    let inspected = api.payments().get(&created.id).send().await?.payment;
    assert_eq!(inspected.id, created.id);

    let mut pages = api.payments().list().set_limit(25u32)
        .pages(Resume::Start, NonZeroUsize::new(10).unwrap());
    let mut first_id = None;
    while let Some(page) = pages.next_page().await? {
        if first_id.is_none() {
            first_id = page.items.first().map(|payment| payment.id.clone());
        }
    }
    if let Some(id) = first_id {
        let payment = api.payments().get(id).send().await?.payment;
        println!("{} {}", payment.amount_money.amount, payment.amount_money.currency);
    }
    Ok(())
}
```

The declared default is `https://connect.squareupsandbox.com`; replacing the base
keeps the declared `/v2` prefix. `bearer` takes the token alone. The generated
config also accepts a native header, so use `bearer` for credential validation.
The helper transport disables redirects and reqwest retries. With an arbitrary
caller transport, one SDK attempt can contain transport-level replays or hops.
`PAYMENTS_WRITE` is needed for create/complete/cancel and `PAYMENTS_READ` for get/list.

Create keys are caller-supplied JSON fields, never headers or SDK-generated IDs.
This slice enforces a conservative 1–45 UTF-8 byte key bound, a nonempty token
source, positive integer minor units, and three uppercase currency letters.
The body is immutable and cloneable; the provider checks source and currency
compatibility. It always sends `autocomplete: true`; callers still inspect the
returned status. Cash, external and card-on-file sources are excluded.

A timeout can occur after a charge commits. `create_outcome_unknown()` flags
create transport failures, unreadable successes, 5xx and redirects conservatively.
Retain the operation/body yourself, including across process restarts if needed.
Replaying it requires the **same key and exact body**. A changed request with that
key is an error: `Error::Provider(p)` and `p.is_idempotency_mismatch()` identify
`IDEMPOTENCY_KEY_REUSED`. The SDK neither creates a new key on failure nor keeps
a persistent key registry. Square decides replay identity. Every SDK `send()`
executes once; HTTP errors, auth failures and timeouts are never automatically
retried. The example's single timeout replay is an explicit caller policy.

`Error::Concord` preserves build, transport, media metadata, decode and limit
stages, operation identity and any response head. `Error::Provider` represents
every non-2xx status: authentication/permission, invalid input or idempotency,
payment-method rejection, not found, throttling, server failure and unknown HTTP
errors. It keeps status/headers even for an empty, malformed, truncated,
oversized or wrongly labelled error body. `document` is either typed Square
errors or a Concord read/decode error. Unknown category/code strings and multiple
entries are retained. Status remains authoritative; no automatic recovery follows
from any category. Success documents are limited to 1 MiB, error documents to
64 KiB. Both use strict JSON media checks.

## Delayed capture

`CreatePayment::authorization(key, source, amount)` validates exactly like `new`
and sends `autocomplete: false`: Square holds the payment `APPROVED`. The key/body
replay rule above applies unchanged. No `delay_duration`/`delay_action` is sent,
so Square's defaults apply: an uncaptured authorization is cancelled after 36
hours (card present) or 7 days (card not present). `CreatePayment::new` still
sends `autocomplete: true`.

`payments().complete(id, CompletePayment::expecting(token))` captures only the
version the caller read (`Payment::version_token`). If the payment changed since,
Square refuses with `VERSION_MISMATCH`: `Error::Provider(p)` with
`p.is_version_mismatch()`. Nothing was captured; re-read the payment and decide
again. The SDK never refreshes the token. `CompletePayment::unconditional()`
sends `{}` and completes whatever version is current. `payments().cancel(id)`
voids an `APPROVED` payment and sends no body. Both work only on an `APPROVED`
payment; other refusals (`DELAYED_TRANSACTION_CAPTURED`, `..._CANCELED`,
`..._EXPIRED`) stay ordinary provider errors in `document`.

Complete and cancel carry no idempotency key. A timeout, unreadable success,
5xx or redirect is `error.outcome_unknown()`: re-read the payment with
`payments().get(id)` and act on its `status` (`APPROVED`, `COMPLETED`,
`CANCELED`). The SDK sends each complete/cancel once and never resends it.
`create_outcome_unknown()` stays true only for create, so a replay rule written
for create does not fire for complete/cancel.

```no_run
use concord_sdk_square_payments::{CompletePayment, CreatePayment, Error, Money, Square};

async fn ship(api: &Square) -> Result<(), Box<dyn std::error::Error>> {
    let body = CreatePayment::authorization("order-1001", "collected-token", Money {
        amount: 4200, currency: "USD".into(),
    })?;
    let held = api.payments().create(body).send().await?.payment; // APPROVED
    let completion = match &held.version_token {
        Some(token) => CompletePayment::expecting(token.clone()),
        None => CompletePayment::unconditional(),
    };
    match api.payments().complete(&held.id, completion).send().await {
        Ok(done) => assert_eq!(done.payment.status, "COMPLETED"),
        Err(Error::Provider(p)) if p.is_version_mismatch() => {
            let fresh = api.payments().get(&held.id).send().await?.payment;
            // Changed since checkout: decide on `fresh`, then complete its token.
            println!("{} {:?}", fresh.status, fresh.version_token.is_some());
        }
        Err(e) if e.outcome_unknown() => {
            let current = api.payments().get(&held.id).send().await?.payment;
            println!("{}", current.status); // COMPLETED if the capture happened
        }
        Err(e) => return Err(e.into()),
    }
    Ok(())
}
```

## Listing

For one page use `list().maybe_cursor(cursor).send()`. For traversal use
`list().pages(from, max_pages)`: `Resume::Start` or `Resume::At(cursor)` explicitly
replaces any template cursor. An empty page with a cursor continues; absent or
empty cursors finish. A bound is `PagesError::Bounded { resume, .. }`; a failure
is `PagesError::Provider { error, failed, .. }`. Resume with the same account,
base and limit. A caller retaining already returned pages keeps them on failure;
`collect_complete()` can instead return the prefix with the error. The page bound
also bounds a looping provider cursor. Square listings are eventually consistent,
not snapshots; changing records can cause duplicates or gaps. This slice leaves
Square's time-range and main-location defaults in effect and exposes only cursor
and limit. Square's default/max page size is 100; larger limits use its default.

## Diagnostics and scope

Debug/Display hide the Bearer token, create and complete bodies (including the
version token), cursor, sensitive payment path and all error entry strings.
Explicit body serialization, native request/URL, header/document access and
payment model fields (including `version_token`) are unredacted. Concord and
pagination types are re-exported for downstream callers. `prepare()` remains the
ordinary Concord request; executing `prepare()?.send()` bypasses the SDK error
policy and returns Concord's status error instead of `Error::Provider`.

Excluded: card collection, OAuth acquisition/refresh, refunds, catalog, other
create options (including `delay_duration`, `delay_action`, partial
authorization), UpdatePayment, CancelPaymentByIdempotencyKey, production
workflows, automatic rate admission, universal write retries and persistent
reconciliation. The SDK uses no native request rewrite or alternate HTTP client.

Verification commands (one Cargo process at a time, default worktree `target/`):

```sh
CARGO_BUILD_JOBS=4 cargo test --locked -p concord-sdk-square-payments
CARGO_BUILD_JOBS=4 cargo clippy --locked -p concord-sdk-square-payments --all-targets -- -D warnings
CARGO_BUILD_JOBS=4 RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-sdk-square-payments --no-deps
```

See [PROVENANCE.md](PROVENANCE.md) for exact sources and fixture derivations,
[FRICTION.md](FRICTION.md) for boundary decisions, [VERIFICATION.md](VERIFICATION.md)
for recorded results, and [MAINTENANCE.md](MAINTENANCE.md) for the delayed-capture
change.
