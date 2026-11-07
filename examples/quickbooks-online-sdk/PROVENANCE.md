# Provenance

Live provider calls: **NOT_RUN**. No request was sent to any Intuit API host
(`quickbooks.api.intuit.com`, `sandbox-quickbooks.api.intuit.com`, `oauth.platform.intuit.com`).
Every expectation below comes from the documentation listed here, or is labelled synthetic or
inferred.

## Documentation read (all on 2026-11-07)

How each page was read matters for how literally its facts can be taken:

- **fetch summary**: a fetch tool that returns a model's summary of the page, not its text.
  Facts are recorded as summarized, with short phrases only.
- **rendered text**: the page opened in a browser and its rendered text extracted; facts were
  read from the page text itself (the API reference pages are client-rendered, a plain HTTP
  fetch returns an empty shell).

| # | Page | Read as | Facts taken |
|---|---|---|---|
| D1 | https://static.developer.intuit.com/output_html/qbo/docs/develop/authentication-and-authorization/oauth-2.0.html (slice citation; title "Set up OAuth 2.0") | fetch summary, two passes | Authorization request to `https://appcenter.intuit.com/connect/oauth2` with `client_id`, `scope` (space-delimited, `com.intuit.quickbooks.accounting`), `redirect_uri` (must match exactly), `response_type=code`, `state` (returned unchanged). Callback parameters `code`, `state`, `realmId` ("the unique ID of the connected user's QuickBooks Online company"); errors `access_denied`, `invalid_scope`. Token endpoint `POST https://oauth.platform.intuit.com/oauth2/v1/tokens/bearer` with `Accept: application/json`, `Authorization: Basic base64(client_id:client_secret)`, `Content-Type: application/x-www-form-urlencoded`; code grant body `grant_type=authorization_code&code=..&redirect_uri=..`; "Only send one request to exchange the authorization code. Multiple requests may invalidate tokens." Refresh body `grant_type=refresh_token&refresh_token=..`. Response fields `token_type` (`bearer`), `expires_in` (3600), `refresh_token`, `x_refresh_token_expires_in` (8640000), `access_token` (and `x_refresh_token_hard_expires_in` only on request). Access tokens valid 60 minutes; refresh tokens have a rolling expiry of 100 days; "Always store the latest `refresh_token` value from the most recent API server response"; when it expires users must reauthorize. API calls send `Authorization: bearer {AccessToken}`. Not on the page: any token-endpoint error code (`invalid_grant`), any explicit rotation rule, any API base URL. |
| D2 | https://developer.intuit.com/app/developer/qbo/docs/api/accounting/all-entities/customer (slice citation) | rendered text | Read a customer `GET /v3/company/<realmID>/customer/<customerId>`; Query a customer `GET /v3/company/<realmID>/query?query=<selectStatement>` (sample `select * from Customer Where Metadata.LastUpdatedTime > '2015-03-01'`); production base `https://quickbooks.api.intuit.com`, sandbox base `https://sandbox-quickbooks.api.intuit.com`. Read response `{"Customer": {..}, "time": ..}`; query response `{"QueryResponse": {"Customer": [..], "startPosition": 1, "maxResults": ..}, "time": ..}` (its sample lists 2 customers with `maxResults: 6`). Customer attributes: `Id` (String, read only, filterable), `SyncToken` (String), `DisplayName` (max 500 chars, filterable, sortable, unique across customers, vendors and employees; no colon, tab or newline), `GivenName`, `FamilyName`, `CompanyName`, `PrimaryEmailAddr` (`Address`), `Active` (Boolean, filterable), `Balance` (Decimal), `CurrencyRef`, `MetaData` (`CreateTime`, `LastUpdatedTime`). |
| D3 | https://developer.intuit.com/app/developer/qbo/docs/api/accounting/all-entities/invoice (slice citation) | rendered text | Create `POST /v3/company/<realmID>/invoice`, `Content type: application/json`; an invoice must have at least one `Line` and a populated `CustomerRef`; `DocNumber` assigned when not supplied; minimum request body: `Line` of `{"DetailType": "SalesItemLineDetail", "Amount": .., "SalesItemLineDetail": {"ItemRef": {..}}}` and `CustomerRef: {"value": ..}`; `CurrencyRef` required only with multicurrency. Read `GET /v3/company/<realmID>/invoice/<invoiceId>`. Response `{"Invoice": {..}, "time": ..}` with `Id`, `SyncToken`, `DocNumber`, `TxnDate`, `DueDate`, `TotalAmt`, `Balance`, `CustomerRef` (`value`, `name`), `Line` (`Id`, `LineNum`, `Amount`, `DetailType`, `Description`, `SalesItemLineDetail` with `ItemRef`, `Qty`, `UnitPrice`, `TaxCodeRef`; a `SubTotalLineDetail` line), `MetaData`. |
| D4 | https://developer.intuit.com/app/developer/qbo/docs/develop/troubleshooting/error-codes | rendered text | 200 "may contain a `<Fault>` element that identifies the error"; 400 bad request; 401 "token ... expired or revoked"; 403 restricted; 404 not found; 500 resubmit once; 503 unavailable. Fault format `<Fault type=..><Error code=.. element=..><Message/><Detail/></Error></Fault>` (XML only on the page); types `ValidationFault`, `SystemFault`, `AuthenticationFault`, `AuthorizationFault`; example errors 2050 `firstname` and 2080 `postalcode`. Codes used: 600 Duplicate Request ID, 610 Object Not Found, 2030 IDs must be valid numbers, 2130 invalid request ID format, 3200 / 401 access token invalid or expired, 5020 Permission Denied, 6000 business validation, 6070 Amount vs Qty × UnitPrice, 10000 application error; a change-order table lists `429` rate limit exceeded. |
| D5 | https://developer.intuit.com/app/developer/qbo/docs/learn/rest-api-features | rendered text | URI formats (`POST baseURL/v3/company/realmId/resourceName`, `GET ../resourceName/entityID`, `GET ../query?query=selectStmt`); request headers `Accept: application/json`, `Authorization` required, `Content-Type: application/json` for POST; example POST to `/v3/company/12345678/invoice` with `Authorization: Bearer ..`; response header `intuit_tid` ("log it"), example `Content-Type: application/json;charset=UTF-8`. |
| D6 | https://developer.intuit.com/app/developer/qbo/docs/learn/learn-basic-field-definitions | rendered text | Realm IDs are in the URI of every request and come from the ID token or the `realmId` redirect parameter; entity IDs are treated as strings. Request ID: query parameter `requestid`; "strongly recommend" for writes; a repeated request with the same ID gets "the same response for the original request"; unique per realm; at most 50 characters (36 for batch); worked example `invoice?requestid=4957`. `DocNumber`: up to 21 characters, not an idempotency key; query by `DocNumber` before retrying a create. |
| D7 | https://developer.intuit.com/app/developer/qbo/docs/learn/explore-the-quickbooks-online-api/minor-versions | rendered text | Minor versions 1–74 discontinued from August 1, 2025; a lower value is ignored and the default is 75; `?minorversion=<#>` query parameter. |
| D8 | https://developer.intuit.com/app/developer/qbo/docs/learn/explore-the-quickbooks-online-api/data-queries | rendered text | Select syntax `SELECT * FROM Entity [WHERE ..] [ORDERBY ..] [STARTPOSITION n] [MAXRESULTS m]`; no `OR`, projections, joins; `LIKE` wildcard `%` only; `Id` supports `=` and `IN` only; values case-insensitive; apostrophe escaped with a backslash; `Active IN (true, false)` for active and inactive customers; at most 1,000 entities per response, 100 by default; `STARTPOSITION`/`MAXRESULTS` pagination (1-10, 11-20, ...); "Server responses only return attributes with values"; a query error puts a `Fault` inside `QueryResponse` (XML example: `type="Validation"`, codes 100 and 300, `element="query"`). GET with the statement as a URI parameter, or POST with `content-type: application/text`. |
| D9 | https://developer.api.intuit.com/.well-known/openid_sandbox_configuration (linked from D1) | fetch summary | Sandbox discovery: `authorization_endpoint` `https://appcenter.intuit.com/connect/oauth2`, `token_endpoint` `https://oauth.platform.intuit.com/oauth2/v1/tokens/bearer` (the same as production's in D1). |
| D10 | https://www.rfc-editor.org/rfc/rfc6749 §5.2 and §6 | fetch summary | Token error responses are HTTP 400 unless specified otherwise, 401 for `invalid_client` with header authentication; `application/json` with `error`; `invalid_grant` covers a refresh token that is invalid, expired or revoked. §6: the server MAY issue a new refresh token, and then the client MUST discard the old one; the server MAY revoke the old one. |

One web search (query: QuickBooks `requestid` idempotency) was used only to find D6; its result
snippets are not a source.

## Rules this SDK adds or infers

| Rule | Status |
|---|---|
| A refresh token rejected with 400 or 401 (`invalid_grant`, `invalid_client`, or no readable document) is terminal for the connection until reauthorization or replacement | SDK policy over D1 (reauthorize when the refresh token expires) and D10 (`invalid_grant` = invalid/expired/revoked, the old token may be revoked after rotation) |
| No response, a 5xx or an unusable 2xx from the token endpoint leaves the refresh token possibly rotated: the latest token received is kept and tried by the next call | **inferred** from D1 ("always store the latest") and D10 §6; not documented by Intuit |
| A refresh token found in a contract-breaking 2xx is stored | from D1 ("always store the latest ..."), conservative |
| `token_type` must be `bearer` (case-insensitive), `expires_in` positive, `refresh_token` non-empty | SDK policy over D1's documented response |
| The access token is refreshed 60 s before `expires_in` elapses | SDK policy |
| One connection = one realm: its path and its token come from the same value; tokens are cached per connection | SDK design for the slice rule; D6 (realm in every URI) |
| A read answered 401 is sent once more with a newly acquired token; a create answered 401 is not | SDK policy; D4 (401 = expired or revoked token). The create rule follows the slice ("never replayed automatically") |
| A create with no response, a 5xx or an unreadable 2xx is uncertain | **inferred**, conservative (D4 says resubmit a 500 once; this SDK leaves that to the caller with the same request ID, per D6) |
| A 2xx whose body is a `Fault` (top level, or inside `QueryResponse`) is a provider error, for a create too | from D4 and D8; the JSON spelling is an inference (below) |
| Every create carries a `requestid` (32 hex characters by default) | SDK policy from D6's recommendation; D6 allows 50 characters, the SDK restricts the alphabet to letters, digits, `-`, `_` |
| Realm and entity IDs are 1–32 ASCII digits | from D4 (2030: IDs must be valid numbers) and D6 (realm IDs are numbers); the length bound is SDK policy |
| `CustomerQuery` refuses `%`, `\` and control characters and escapes `'` | from D8 (only `'` has a documented escape; `%` is the wildcard) |
| `MAXRESULTS` 1–1000, sent explicitly (default 100); `STARTPOSITION` one-based; a page shorter than `MAXRESULTS` is the last; a reported `startPosition` must equal the requested one | from D8; the stop rule and the echo check are SDK policy. `maxResults` is not checked against the count: D2's own sample disagrees with its list |
| An empty result is `{"QueryResponse": {}}` | **inferred** from D8 ("only return attributes with values"); not shown on any page |
| `minorversion=75` on every Accounting request | from D7 |
| Amounts, quantities and prices are kept as decimal text, never `f64` | SDK policy |

**JSON spelling of faults.** D4 and D8 show faults only in XML. The fixtures write them as
`{"Fault": {"Error": [{"Message": .., "Detail": .., "code": .., "element": ..}], "type": ..}}`:
element names become capitalized keys, attributes lowercase keys. This spelling is an
**inference**, not a documented JSON example; a different real spelling would make fault
documents unreadable (the status is still reported).

## Fixtures

Every fixture is authored for these tests. "Shape" means the field names and nesting follow the
cited page; values are synthetic unless the row says otherwise. No documentation JSON example
was copied whole.

| Fixture | Derivation | Used by |
|---|---|---|
| `customer-query-page1.json` | synthetic; shape D2 (query response, Customer fields), 2 customers, `startPosition` 1, `maxResults` 2 | protocol (query), workflows (fake's customers) |
| `customer-query-page2.json` | synthetic; shape D2, 1 customer at `startPosition` 3 | protocol (offset echo), workflows |
| `customer-query-empty.json` | synthetic; inferred empty `QueryResponse` (D8) | protocol |
| `customer-read.json` | synthetic; shape D2 (read response) | protocol, token-class tests |
| `customer-malformed.json` | synthetic; truncated JSON | protocol |
| `customer-wrong-shape.json` | synthetic; a customer without `Id` | protocol |
| `invoice-create-request.json` | derived from D3's minimum create request (`Line` with one `SalesItemLineDetail` of `Amount` 100.0 and `ItemRef.value` "1", `CustomerRef.value`); the customer ID is synthetic ("29"), `ItemRef.name` omitted | protocol (expected request body) |
| `invoice-created.json` | synthetic values; shape D3 (create response: lines, subtotal line, `TotalAmt`, `CustomerRef`) | protocol |
| `invoice-read.json` | synthetic values; shape D3 (read response with `Qty`, `UnitPrice`, `TaxCodeRef`) | protocol |
| `fault-validation.json` | JSON transcription (spelling inferred, above) of D4's XML example: codes 2050/`firstname`, 2080/`postalcode`, type `ValidationFault` | protocol, fake (wrong minor version) |
| `fault-in-success.json` | synthetic; D4 code 6000, sent with 200 (D4: a 200 may carry a fault) | protocol |
| `query-fault-in-success.json` | JSON transcription of D8's XML query fault (type `Validation`, codes 100 and 300, `element` `query`) | protocol |
| `fault-authentication.json` | synthetic; D4 code 3200 with type `AuthenticationFault`, sent with 401 | protocol, fake |
| `fault-authorization.json` | synthetic; D4 code 5020, type `AuthorizationFault`, sent with 403 | protocol |
| `fault-object-not-found.json` | synthetic; D4 code 610. The HTTP status QuickBooks pairs with 610 is not documented: the tests send it with 400 | protocol, fake |
| `fault-system.json` | synthetic; D4 code 10000, type `SystemFault`, sent with 500/503 (and 429 in one write test) | protocol, fake |
| `token-issued.json` | synthetic values; shape D1 (token response) | protocol |
| `token-wrong-type.json` | synthetic; D1 shape with `token_type` `mac` | protocol |
| `oauth-invalid-grant.json` | D10 §5.2 (`{"error": "invalid_grant"}`); Intuit documents no token error body | protocol, fake |
| `oauth-invalid-client.json` | D10 §5.2 (`invalid_client`, 401) | protocol, fake |

The stateful fake in `tests/support/mod.rs` is synthetic code implementing the rules above:
tokens per realm, an access token accepted only on its own realm's paths, refresh rotation with
the replaced token refused as `invalid_grant`, one-time authorization codes, `minorversion=75`
required, `DisplayName = '..'` the only filter applied, `STARTPOSITION`/`MAXRESULTS` slicing, and
`requestid` deduplication per realm (D6).

## Crates

No new crate: every dependency (`concord` with its `form` feature, `concord-auth`,
`concord-pagination`, `serde`, `serde_json` with `raw_value`, and for tests
`concord-example-testserver` and `tokio`) is a workspace member or was already in `Cargo.lock`.
The only `Cargo.lock` change is this package's own entry; no other package changed version.
