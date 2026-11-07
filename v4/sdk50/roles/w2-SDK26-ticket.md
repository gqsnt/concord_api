Callers need to send several transactional emails in one request. Postmark's Email API documents
batch sending (`POST /email/batch`, up to 500 messages per call; read the Email API page and cite
the section and date). Its successful HTTP response carries one result per message, each with its
own `ErrorCode` and `Message`: some messages can be accepted while others are rejected in the
same 200 response. Add batch sending to the SDK so a caller can tell, per message, accepted (with
its message id) from rejected (with the provider's code, without leaking provider text in
`Display`/`Debug`), and so a batch whose response is lost or unreadable is reported as an unknown
outcome for the whole batch and never resent. Keep single-message sending, listing and details
unchanged.
