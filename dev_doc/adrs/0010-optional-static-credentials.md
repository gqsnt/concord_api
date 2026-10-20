# ADR 0010 — Defer optional initial static secrets

- **Status:** Superseded for v3 by [ADR 0020](0020-optional-static-secrets-remain-rejected.md): the rejection becomes a selected v3 contract rather than a pending deferral; the invariant below is carried forward.
- **v2.1 status (historical):** Deferred for v2.1

## Decision

The v2.1 grammar accepts only required `secret name;` static input and rejects
postfix optional-secret punctuation with a focused diagnostic. Do not infer a
private authority lifecycle from public value optionality.

Supported v2.1 alternatives are an endpoint-backed slot that naturally starts
unavailable, or a required custom provider whose acquisition can return
credential-unavailable (`AuthErrorKind::Unavailable`). A separate
least-authority client/profile model is not added without real API evidence.

## Property earned

The implemented private versioned-slot architecture stays truthful without an
unimplemented construction surface or silently optional placement.

## Invariant

If an endpoint declares a placement and material is unavailable, execution
fails before dispatch. It never drops auth.

## Revisit

Revisit only when a representative multi-auth API requires one client to serve
public and mutually exclusive authenticated surfaces and the required-provider
or endpoint-backed alternatives are materially inadequate. Any revisit must
prototype the complete private-state lifecycle and obtain human/product
evidence; punctuation alone is not a capability model.
