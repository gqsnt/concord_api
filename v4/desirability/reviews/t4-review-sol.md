# Review t4-review-sol (CORRECTNESS REPAIR review, T4, on a613d276)

Reviewer: Codex gpt-6.1-sol, xhigh, read-only (observed from rollout turn_context),
2026-11-09T00:56:31Z–00:59:05Z, thread 01a1bdce-88d2-7870-a737-bfb4aca96b7b.
(T4 authored by a Codex lane: independent session, same provider.)
**Verdict: ACCEPT_WITH_FIXES** (1 non-blocking). Confirmed: patch = commit; both switch directions fail
before and pass after for customer reads and invoice writes; same-environment and fixture controls pass;
token-source assertions, fixture wire equality, token-free `prepare()` with the base snapshot; exact
HTTPS-host matching, port/path/query/request fields preserved, proxy configuration and non-official
destinations unchanged, selection before token acquisition, retry cloning after selection; existing tests
unchanged; the localized repair restores the documented invariant without new public concepts, setter
errors or preparation changes — preferable to the listed alternatives; 49 crate tests, 4,000 workspace
tests.

| # | Finding | Severity | Triage |
|---|---|---|---|
| 1 | "replaces all three together at send time" misstates timing: the setter replaces the connection immediately; only host selection and authentication happen at send (`lib.rs:10`) | non-blocking | fixed in 9f7a3336 (coordinator, doc wording) |
