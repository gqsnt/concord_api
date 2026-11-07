Callers need to tag sends and attach custom data for later event searches. Mailgun's Messages API
documents `o:tag` (up to 3 tags per message, sent as a repeated form field) and custom variables
`v:<name>` (JSON values, returned in events as `user-variables`) — read the send-message reference
and the tagging / custom-variables pages, cite them with the date. Add both to sends so they work
with in-memory and with streamed (non-rewindable) attachments alike, refuse a fourth tag or an
invalid variable name before any I/O, and let a caller filter domain events by tag. Keep the
existing send, events paging and delivery-status behavior unchanged.
