Callers need to record payments against invoices. In the Accounting API a `Payment` entity is
created with `POST /v3/company/{realmId}/payment` (a customer reference, a total amount, and
`Line` entries whose `LinkedTxn` names the invoice being paid); reading the invoice afterwards
shows its reduced `Balance`. Read the Payment entity reference and the `requestid` guidance and
cite them with the date:
- https://developer.intuit.com/app/developer/qbo/docs/api/accounting/all-entities/payment
- https://developer.intuit.com/app/developer/qbo/docs/develop/rest-api-features

Add: recording a payment for one invoice (amount paid; partial payments allowed) and reading the
invoice's balance afterwards, within the connection's realm only. Payment creation follows the
same rule as invoice creation: it always carries a `requestid`, is never resent by the SDK, and a
timed-out create is uncertain with that request id. Amounts keep their exact decimal text. Keep
customer query/read and invoice create/read unchanged for existing callers.
