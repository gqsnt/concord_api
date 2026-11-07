# concord-sdk-quickbooks-online

A bounded QuickBooks Online Accounting API SDK built on Concord: realm-bound OAuth 2.0
connections (authorization code and refresh token), customer query and read, invoice create and
read. Every test runs against local fixtures; **no request was ever sent to Intuit**
(live provider calls: NOT_RUN).

## What the slice covers

| Need | Call | Wire |
|---|---|---|
| Send the user to Intuit | `app.authorization_url(state)` | `https://appcenter.intuit.com/connect/oauth2?client_id=..&response_type=code&scope=com.intuit.quickbooks.accounting&redirect_uri=..&state=..` |
| Connect a company (realm) | `AuthorizationCallback::from_query(q)?`, `app.exchange_code(&http, callback, state)` | `POST https://oauth.platform.intuit.com/oauth2/v1/tokens/bearer`, Basic client credentials, form `grant_type=authorization_code&code=..&redirect_uri=..` |
| Resume a stored connection | `app.connection(realm, refresh_token)` | refreshes on first use: form `grant_type=refresh_token&refresh_token=..` |
| A client for that company | `QuickBooks::connect(http, connection)` | `https://sandbox-quickbooks.api.intuit.com` or `https://quickbooks.api.intuit.com`, `/v3/company/{realmId}/..?minorversion=75` |
| Query customers | `qbo.customers().query(CustomerQuery)`, `customer_pages(&qbo, query)` | `GET /query?query=SELECT * FROM Customer .. STARTPOSITION n MAXRESULTS m` |
| Read a customer | `qbo.customer(id).read()` | `GET /customer/{id}` |
| Create an invoice | `qbo.invoices().create(NewInvoice)` | `POST /invoice?requestid=..`, JSON `{"Line": [..], "CustomerRef": {"value": ..}}` |
| Read an invoice | `qbo.invoice(id).read()` | `GET /invoice/{id}` |

The client hierarchy is Concord's: client (`QuickBooks`, one company) → scope (`customers`,
`customer(id)`, `invoices`, `invoice(id)`) → endpoint. Every call is the generated `send()`; the
client's send policy is the `Connection`, which supplies both the realm of the path and its
access token.

## How to call it

Both connected workflows, after an authorization (the code is compiled, not run):

```rust,no_run
use concord_sdk_quickbooks_online::{
    Activity, AuthorizationCallback, CustomerQuery, Decimal, EntityId, Environment, Error,
    NewInvoice, OAuthApp, QuickBooks, SalesItemLine, http_client,
};

async fn connect_and_bill(callback_query: &str, state: &str) -> Result<(), Error> {
    let http = http_client().expect("a transport");
    let app = OAuthApp::new(
        Environment::Sandbox,
        "client-id",
        "client-secret",
        "https://app.example/oauth/callback",
    )?;
    // The user went to `app.authorization_url(state)?`; Intuit redirected back with
    // `code`, `state` and `realmId`.
    let callback = AuthorizationCallback::from_query(callback_query)?;
    let connection = app.exchange_code(&http, callback, state).await?;
    let qbo = QuickBooks::connect(http, connection.clone());

    // Workflow 1: query customers, then read the selected one in the same realm.
    let query = CustomerQuery::new()
        .activity(Activity::Active)
        .order_by_display_name()
        .max_results(50)?;
    let page = qbo.customers().query(query).send().await?.page;
    let Some(selected) = page.customers.first() else {
        return Ok(());
    };
    let customer = qbo.customer(selected.id.clone()).read().send().await?.customer;

    // Workflow 2: create a sandbox invoice for that customer, then retrieve it.
    let line = SalesItemLine::new(Decimal::from_cents(10_000), EntityId::new("1")?);
    let invoice = NewInvoice::new(customer.id.clone(), line);
    let created = qbo.invoices().create(invoice).send().await?.invoice;
    let fetched = qbo.invoice(created.id.clone()).read().send().await?.invoice;
    assert_eq!(fetched.customer_ref.value, customer.id.as_str());

    // Persist the latest refresh token: QuickBooks may return a new one on any refresh.
    let _store = (connection.realm().clone(), connection.refresh_token());
    Ok(())
}
```

`customer_pages(&qbo, query)` walks a query page by page (`concord_pagination::Pages`): it ends
only after a page shorter than `MAXRESULTS`; `max_pages(n)` caps it.

Use `http_client()` or a `reqwest::Client` with the same restrictions: **no transport retries**
(a create must be sent exactly once) and **no redirects** (no credential follows a hop). The SDK
never builds its own client.

## Failure semantics

Every error is the SDK's `Error`; none prints a token, a secret, an authorization code, a URL,
a provider message or a body.

| Situation | Result |
|---|---|
| A replaced, expired or revoked refresh token (token endpoint `400`/`401`, e.g. `invalid_grant`) | `Error::Authorization` with `kind() == GrantRejected`, `realm()` = this connection's realm, `needs_reauthorization() == true`. **Nothing is sent to the Accounting API**, no other realm's token is used or changed, and the connection stops asking for tokens until it is recovered. |
| Recovering it | `connection.reauthorize(&http, callback, state)` with a new authorization of the **same** realm (a callback for another realm is `CallbackError::RealmMismatch`, unsent), or `connection.replace_refresh_token(token)` with a newer token of this realm. Every clone of the connection, and every client built on it, works again. |
| The token endpoint gave no usable answer (no response, a 5xx, an unreadable or contract-breaking 2xx) | `GrantUncertain`: the latest refresh token the connection received is kept (a refresh token found in a contract-breaking 2xx is stored); the next call tries again. |
| The token endpoint refused otherwise (429, 403, ...) | `TokenServiceUnavailable`; the next call tries again. |
| A 401 from the Accounting API on a read | the token is discarded, one new token is acquired and the read is sent **once** more; a second 401 is `AccessTokenRejected`. |
| A 401 on a create | `AccessTokenRejected` at once; the create is **not** resent (QuickBooks refused it before processing; sending it again with the same request ID is safe). |
| A create with no response, a 5xx, or an unreadable 2xx | `Error::Uncertain` with the `requestid` it carried. The SDK never resends it. Replay deliberately with the same ID: `qbo.invoices().create(invoice).set_request_id(id)`; QuickBooks answers a repeated request ID with the original response instead of creating a second invoice. |
| Any other non-2xx | `Error::Provider`: status, the `Fault` (type, error codes, elements; messages only through `*_unredacted`), `intuit_tid`. An unreadable fault document keeps the status. |
| A 2xx whose body is a `Fault` (documented) | `Error::Provider` with status 200, for reads and creates (a fault: the create did not happen). |
| A 2xx read that is malformed, truncated, too large (`QUERY_BODY_LIMIT`, `ENTITY_BODY_LIMIT`) or not `application/json` | `Error::Concord` of kind `Decode`, `Transport`, `Limit` or `Metadata`. |
| An input outside its documented range (a realm or entity ID that is not digits, `MAXRESULTS` over 1000, a request ID over 50 characters, a `DocNumber` over 21) | `Error::InvalidInput`; nothing is sent. |

Concurrent calls on one connection share a single refresh. A connection, its refresh token,
its rejection state and its cached access token belong to one realm; the SDK caches no records.

## Not covered

- Payroll, tax workflows, payments, every other entity and operation (update, delete, void,
  send, PDF, batch, change data capture, reports, webhooks).
- The full query dialect: `CustomerQuery` builds only `SELECT * FROM Customer` with `Active`,
  `Id` and `DisplayName` (equals or prefix) filters, `ORDERBY DisplayName`, `STARTPOSITION` and
  `MAXRESULTS`. Values with `%`, `\` or control characters are refused (no documented escape).
- Automatic replay of invoice creation, retries of anything but one read after a 401, rate
  limiting (QuickBooks' throttling answers are returned as `Error::Provider`).
- Multicurrency (`CurrencyRef` on a new invoice), line types other than sales item lines.
- OpenID Connect (ID tokens, user info), token revocation, and persistence: the application
  stores `connection.refresh_token()` itself, after calls.
- Live verification: every expectation comes from the documentation cited in `PROVENANCE.md`
  or is labelled synthetic there.
