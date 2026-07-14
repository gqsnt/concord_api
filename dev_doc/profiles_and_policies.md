# Profiles And Policies

Concord separates reusable declarations from their attachment sites.

Profiles contain authentication and rate-limit attachments. They may extend
other profiles and may be attached in the client `default` block, scopes, or
endpoints. `policies { ... }` declares rate-limit profiles and observers.

The attachment order is:

```text
client default
-> outer scopes
-> inner scopes
-> endpoint
```

Authentication uses append in source order. Rate-limit additions combine,
`rate_limit only NAME` replaces inherited limits, and `rate_limit off` clears
the inherited limit. Rate-limit specifications are resolved at the attachment
site so endpoint and scope key bindings remain available.

Sema rejects duplicate profile names at one attachment site and detects
inheritance cycles. Profile names are retained as generated rustdoc labels in
stable first-seen order. Profile declarations are resolved before codegen and
do not appear as runtime objects.

Header and query authentication targets are validated after inheritance.
Header names compare case-insensitively; bearer, Basic, and custom
`Authorization` placements share one target. Query-auth keys are checked
against public query policy before provider or body side effects.
