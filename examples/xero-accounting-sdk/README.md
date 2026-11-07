# Xero Accounting invoice SDK (SDK35)

`concord-sdk-xero-accounting` implements two connected workflows: page an authorized tenant's invoices and retrieve a chosen invoice; create one draft sales invoice and retrieve its current state in that same tenant. This is an **example-assisted**, bounded SDK over frozen Concord commit `14b9e1e0a89775dbe5cbb9f7837bde677ea5878e`. Live provider calls: **NOT_RUN**. All execution evidence uses local `concord-example-testserver` fixtures.

The declaration in `src/lib.rs` retains client → tenant scope → list/get/create endpoint. Every request, including OAuth refresh, uses a generated Concord `send()`. The private declaration protects credentials and tenant context from arbitrary generated setters. Public operations retain the familiar `.send().await` terminal method.

## Calling the slice

The application completes authorization-code consent, validates OAuth state and its callback, obtains the authorized tenant from Xero's connections, and supplies a grant. Request `offline_access accounting.invoices` for both workflows. A read-only grant may use `offline_access accounting.invoices.read`. Existing broad `accounting.transactions` / `.read` grants are also accepted. Scope selection and authorization are application assertions, not inferred by parsing a JWT.

```no_run
use concord_sdk_xero_accounting::{
    Amount, FileStore, Grant, LineItem, TenantId, Xero,
};
use std::{num::NonZeroUsize, path::Path, time::SystemTime};

async fn workflows(
    journal_directory: &Path, // a NEW directory beneath an application-private parent
    tenant_id: TenantId,     // selected from authorized connections
    client_id: String,
    client_secret: &str,
    access_token: String,
    refresh_token: String,
    access_expires_at: SystemTime,
    contact_id: &str,        // existing contact in this tenant
) -> Result<(), Box<dyn std::error::Error>> {
    let grant = Grant::new(
        tenant_id.clone(), client_id, access_token, refresh_token,
        "offline_access accounting.invoices", access_expires_at,
    )?;
    let store = FileStore::create(journal_directory, grant)?;
    let xero = Xero::new(store, client_secret)?;
    let tenant = xero.tenant(&tenant_id)?;

    let mut pages = tenant.pages(None, 100, NonZeroUsize::new(20).unwrap())?;
    if let Some(page) = pages.next_page().await? {
        if let Some(chosen) = page.items.first() {
            let current = tenant.get(&chosen.reference)?.send().await?;
            assert_eq!(current.reference.tenant(), &tenant_id);
        }
    }

    let contact = tenant.contact(contact_id)?;
    let line = LineItem::new("Consulting", 1, Amount::from_minor_units(1234)?)?;
    let draft = tenant.draft(contact, vec![line])?;
    let created = tenant.create(draft)?.send().await?;
    let state = tenant.get(&created.reference)?.send().await?;
    assert_eq!(state.reference, created.reference);
    Ok(())
}
```

For restart, use `FileStore::open(directory, tenant, client_id)` rather than initializing another grant. For fixtures, `Xero::with_origins(store, secret, accounting_origin, oauth_origin)` accepts HTTPS or loopback HTTP origins. The transport disables redirects, reqwest retries and Referer forwarding, with a ten-second request timeout. No constructor sends a request.

`Tenant::list(page, size)` retrieves one page. `Tenant::pages(from, size, cap)` returns `concord_pagination::Pages`; `next_page()` returns `Ok(None)` only on provider exhaustion. `items().collect_complete()` returns either a complete vector or an `Incomplete { collected, error }` preserving the prefix. A cap, failed page and completion are distinct. `InvoicePage::next` can resume with the same tenant and size. With modern metadata, page, size, count and total must agree. Without metadata, even a short nonempty page continues until an empty page. Mutations during traversal can cause duplicates or gaps; there is no provider snapshot guarantee.

## Credentials and atomic rotation

One `FileStore` belongs permanently to one tenant and OAuth client ID. An `Xero` instance fixes the destination, grant, scopes and tenant for its single-slot `concord-auth` cache. Clones share single-flight acquisition. A delayed 401 invalidates only the generation that was sent; it is returned to the caller without replay, and the next operation attempts refresh. Concurrent independent clients cannot refresh the same journal at once: an exclusive create-new `lock` file serializes ownership across processes. Contention returns a typed error.

The journal uses bounded JSON files, private file modes on Unix (directory 0700, files 0600), file `sync_all`, same-directory rename and directory `sync_all`. It contains plaintext credentials; application storage/access/backup policy remains outside the SDK. Use a private local filesystem supporting these operations; network filesystems and Windows crash behavior are unverified.

Rotation ordering:

1. Acquire the exclusive journal lock. Durably replace the active pair with a `Refreshing` marker **before** issuing the OAuth request. The old refresh token exists only inside the owned acquisition future at this point.
2. Send the form-encoded refresh request once, using sensitive Basic authorization. Require a valid Bearer pair, unchanged scopes when supplied, and a lifetime over the 30-second margin and no more than 1800 seconds.
3. Save the **whole new pair** in `pending.json`, fsync it and its directory, then atomically rename it over `state.json` and fsync the directory again. Only then publish the access token to the cache and send Accounting requests.
4. If publication fails after the complete pending pair is durable, fail the operation. `recover_pending()` can publish that pair without repeating refresh. The fault workflow verifies this using a real failed rename and a restarted client.

There is no atomic transaction spanning Xero's remote token issuance and local disk. A crash or lost response before a complete pair is journaled can lose the new credential. This implementation fails closed: the old token is absent from the active marker and cannot be the sole saved pair silently reused after restart. `Refreshing` without a valid pending pair requires reauthorization into a new directory. A crash-held lock is never automatically stolen; an operator must establish that its owner is gone before manual repair, or authorize a fresh grant. Xero documents a grace-period retry, but this slice deliberately does not use it automatically. Redaction is neither encryption nor memory zeroization.

## Responses, rates and failures

The Accounting API defaults to XML. `Json` sends `Accept: application/json`; create also sends JSON `Content-Type`. This avoids an XML dependency and uses Concord's strict head checks. XML, missing/repeated media labels, malformed/truncated data and bodies over 1 MiB are refused. OAuth and provider error documents are bounded to 64 KiB. `ProviderFailure` retains HTTP status and metadata even if its error document cannot be read. Text-only 503 responses therefore keep `Unavailable` and a media error rather than losing the status.

HTTP classes: 400 validation, 401 unauthorized, 403 forbidden, 404 missing, 412 precondition, 429 rate limited, 500 internal, 501 unimplemented, 503 unavailable (including organisation offline), plus other non-success statuses. A 2xx invoice with validation errors is also a failure. Missing required invoice fields, inconsistent paging, unexpected IDs or a create result other than DRAFT/ACCREC are contract failures. `Problem` fields may contain private text and are available by deliberate access; ordinary `Debug` and `Display` withhold secrets, IDs and document content.

GETs may repeat once after a 429 with a valid singleton `Retry-After`, within a ten-second total attempt budget. The shared parser handles seconds and HTTP dates. The server delay is a minimum, never shortened to fit the budget; absent/invalid/repeated hints are returned without replay. A process-local limiter shares cooldown across clones, refuses work during that cooldown and caps concurrent work at five. It does not predict daily/app quotas or coordinate independent processes' Accounting traffic. No automatic transport, 5xx, decode or 401 replay occurs.

Creates and refresh exchanges send once. A create transport failure, 5xx, unreadable 2xx or inconsistent success is marked `write_uncertain()`: reconcile within the same tenant before considering any manual retry. Explicit invoice validation and non-5xx HTTP refusals are not marked uncertain. Token persistence/acquisition failures occur before Accounting dispatch. A request deadline is conservatively uncertain for creates. The SDK never performs payment, approval, email, voiding or deletion transitions.

Tenant IDs, invoice/contact references, drafts and paging continuations carry bindings. Cross-tenant use fails before I/O. When importing a raw invoice/contact ID, the application explicitly asserts its owning tenant; this slice does not implement connections or Contacts discovery and cannot verify a false application assertion.

## Exclusions and verification

Excluded: Payroll, bank feeds, payment creation, full invoice state machine, updates, bills, attachments, PDFs, tax/account configuration, all other Accounting endpoints, consent/callback handling, and distributed quota scheduling. Amounts are exact nonnegative two-decimal numbers with positive integral quantity; no financial totals are calculated here.

Run sequentially, using this worktree's default `target/`:

```sh
CARGO_BUILD_JOBS=4 cargo test --locked -p concord-sdk-xero-accounting
CARGO_BUILD_JOBS=4 cargo clippy --locked -p concord-sdk-xero-accounting --all-targets -- -D warnings
CARGO_BUILD_JOBS=4 RUSTDOCFLAGS="-D warnings" cargo doc --locked -p concord-sdk-xero-accounting --no-deps
```

See [PROVENANCE.md](PROVENANCE.md) for sources, fixture derivations and example assistance; [FRICTION.md](FRICTION.md) for ownership decisions and proposed Concord improvements; [VALIDATION.md](VALIDATION.md) for observed checks. Local HTTP/1.1 fixtures do not establish live provider, TLS or production filesystem behavior.
