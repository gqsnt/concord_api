# Maintenance: upload location, scopes, interruption and digest failure

The frozen library was left unchanged. The requested behaviors were already implemented before
this maintenance pass: the upload session follows each returned location, tracks uncertainty and
the reported range, token cache keys distinguish pull from push, and digest rejection remains a
typed provider error. No public workflow or caller signature was changed.

## Edits by kind

- **Declaration:** none. `src/api.rs` already declares the PATCH, status GET and completion PUT,
  with private placeholder paths; the adapter follows the returned URL.
- **Adapter:** no behavior change. `src/upload.rs` already adopts the latest location and range,
  requires status after an uncertain request, and appends the digest to the current URL. The
  `src/client.rs` and `src/auth.rs` changes are rustdoc corrections about the caller's redirect
  policy. The client already caches tokens by `Access` and validates returned locations and
  token realms.
- **Model:** none. Existing `ErrorCode::DigestInvalid`, error metadata and upload state express the
  failure and partial progress.
- **Tests:** `tests/workflows.rs` adds a combined pull-then-push scenario: the second PATCH is
  stored but its response is lost, status returns the new location and range, the caller resumes,
  and an incorrect completion digest yields a typed 400 without publishing either digest. It
  checks literal request targets, ranges, distinct synthetic tokens and redaction. The new
  `tests/protocol.rs` case checks an authenticated foreign Location and a foreign redirect against
  a third loopback origin; no request reaches it with redirects disabled. Existing fixtures and
  fake responses remain hand-authored; no fixture bytes changed.
- **Docs:** the README and crate/Registry/auth rustdoc now explain and demonstrate the redirect
  policy needed to keep requests and opaque URL state from crossing origins. `FRICTION.md`
  records the transport boundary and a reproduction.

## Change locality and duplicated facts

No existing declaration, adapter, model, test or document needed a semantic change to implement
the upload perturbation: it was already covered individually and in the existing workflow. The
combined regression adds another independent test description, but did not require changing a
provider contract or its callers. Scope selection remains represented both by `Access::scope` and
by operation call sites choosing pull or push; the fake also independently declares required
scopes. Upload request shape is represented in the declaration and the adapter. Those are
independent descriptions inspected, **zero** of which needed modification for this ticket.

The redirect-policy clarification did touch **six** independently maintained public descriptions:
the README construction example, its construction guidance, its limitations paragraph, the
`Registry` rustdoc, the crate-level example, and the auth module's origin guidance. Each needed
the same caller-owned policy fact. The semantic upload change was already local in the session
adapter, but the security guidance was scattered across these entry points. The test transport's
policy was already correct and did not need a change.

## Reading and observations

Before the first attempt, read this SDK's source, README, provenance, protocol/workflow tests and
test support, plus the root README and Concord's author guide and recipes for `follow` and
redirects. After the first attempt passed, read this SDK's `FIRST_USE.md` and `FRICTION.md` to
compare the observations. No other provider SDK supplied this solution; no held-out campaign
records or other SDK records were read. The existing first-use tests were run unchanged.

There was no generated-name or compilation surprise in this pass; generated declaration methods
were already used by the adapter. The earlier request-factory and placeholder-path workarounds
remain documented in `FRICTION.md` (F3 and F4). A caller-supplied HTTP client can still follow
cross-origin redirects if configured to do so; the SDK cannot enforce the recommended policy on
an arbitrary client. The new redirect test uses the existing `Policy::none()` test transport and
does not establish protection for a different client configuration.

Verification: `CARGO_BUILD_JOBS=5 CARGO_TARGET_DIR=/home/canarit/projects/Concord_Consolidation_SDK20_Handoff/work/target-b cargo test --locked -p concord-sdk-distribution-registry` passed offline, including all first-use, protocol, workflow and doc tests. No live provider, credentials or secrets were used.
