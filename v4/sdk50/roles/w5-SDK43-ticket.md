Callers need to resolve or ignore an issue: Sentry's organization issue endpoint accepts a
`PUT` that updates an issue's status (`resolved`, `ignored`, `unresolved`, with the documented
status details) and returns the updated issue. Read the issue update reference and cite it with
the date:
- https://docs.sentry.io/api/events/update-an-issue/

Add the status update so a caller sends only the change it means and gets the updated issue back;
a status or detail outside the documented set is refused before any request; the write respects
the same frequency and concurrency limits as reads; a timed-out update is uncertain (the caller
re-reads the issue) and is never retried automatically. Keep issue and event listing and
retrieval unchanged for existing callers.
