Callers need an incremental invoice sync: list only the invoices modified since a point in time.
Xero's Accounting API accepts an `If-Modified-Since` request header (a UTC timestamp) on
`GET Invoices`, combined with paging. Read the invoices reference and the requests-and-responses
page and cite them with the date:
- https://developer.xero.com/documentation/api/accounting/invoices
- https://developer.xero.com/documentation/api/accounting/requests-and-responses/

Add a "modified since" listing for the authorized tenant that keeps the existing page traversal
(resume, cap, completion) and keeps the same timestamp across all pages of one traversal, including
a resumed one; a caller can record the time at which a completed sync started and use it for the
next sync. The timestamp must be valid UTC and must never be sent to another tenant. Keep the
existing listing, retrieval and draft-invoice creation unchanged for existing callers.
