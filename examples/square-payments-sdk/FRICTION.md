# Concord friction and ownership

No shared crate was changed. The baseline declaration supports this control's
ordinary client → payments scope → create/get/list operations, body JSON key,
strict decoding and generated `send()` without a native escape. The items below
record costs and proposed dispositions, not claims of core defects.

| Item | Observable evidence / handwritten work | Ownership | Smallest Concord change or disposition |
| --- | --- | --- | --- |
| F1: typed non-success documents | `SquarePolicy::send` branches on status, calls bounded `document`, wraps metadata and its Result; `protocol` tests preserve HTTP status through every body fault | Square envelope/error semantics belong in SDK; branching/bounded document mechanics are generic | Optional shared bounded status-document projection helper could remove the branch/wrapping ceremony. Keep error model/status policy in SDK; existing Received already handles the difficult read mechanics. No new declaration syntax is needed for three endpoints. |
| F2: replay identity and uncertain create | Caller must retain an operation clone. Workflow ledger proves committed-first timeout, byte-identical replay and changed-body refusal. SDK writes `create_outcome_unknown` using operation/delivery/head facts. | Provider replay rule and application persistence, not Clone semantics | Retain current Endpoint/Delivery boundaries. No automatic replay inference from POST/Clone/status. A durable key registry would enlarge and misplace this slice. |
| F3: cursor envelope projection and bounds | `SquarePaymentsList::pages` clones generated template, replaces cursor, moves records and maps empty cursor to None; caller supplies Resume and mandatory cap. Tests cover empty pages, cap/resume and failed cursor. | Empty-cursor meaning is Square; scheduling, checkpoints and redacted diagnostics use shared pagination | No core change needed; provider projection remains explicit. A context-bound continuation could catch account/query mismatch, but would add state/types to this simple control. Current API documents same-template resumption. |
| F4: transport replays and redirects | Small `transport` constructor disables reqwest retry/redirect; tests assert exact dispatch counts and unfollowed 307 | Transport configuration is application/SDK setup; one Concord execute cannot attest wire count for an opaque supplied client | Retain transport ownership. A documented opt-in no-replay transport recipe suffices; do not silently change caller clients. |
| F5: input and model diagnostics | Create body validation and redacted Debug, open payment status, withheld error category/code/detail/field, cursor-safe page Debug | Provider models/input restrictions and SDK diagnostic policy | No shared change needed. Concord already withholds bodies and sensitive placements. Deriving Debug on SDK-owned secrets would still leak them outside Concord. |

Shared capability decisions after reading their READMEs:

- `concord-auth`: use `bearer` for checked sensitive headers; no token cache,
  refresh or auth replay for a caller-supplied static token.
- `concord-pagination`: use `Pages`, `Page`, `Resume`, mandatory `max_pages` and
  typed failure/continuation. Repeated cursors are bounded, not deduplicated.
- `concord-codecs`: use facade `json` (including `extras`) and standard strict
  Json; no custom codec, unchecked media adapter or streaming formats needed.
- `concord-retry`: not selected. The required replay is an explicit caller action
  with unchanged body/key; no automatic retry scheduler is required. Adding one
  would expand failure policy without simplifying this control. Its README's
  distinction between repeatable bytes and semantic permission is retained.
- `concord-ratelimit`: not selected. Return 429 and its metadata as a provider
  error. No quota/window contract or background admission is promised by this
  slice; process-local limiting would add state. Applications can apply their own
  rate/backoff policy without changing a replay's body/key.

Examples consulted: **no `examples/*-sdk` file was opened**. This is
documentation/example-excerpt assisted, not a docs-only blind first use.

| Actual file read | Example content consulted and takeaway |
| --- | --- |
| Root `README.md` | Embedded `examples/quickstart/src/lib.rs` snippet: scope navigation, JSON bodies, sensitive Bearer, bounded error document |
| `docs/QUICKSTART.md` | Embedded quickstart lib/main/local-server snippets and shapes wire excerpt: client/config, with_base_url and fixture base prefix |
| `docs/AUTHOR_GUIDE.md` | Embedded quickstart declaration patterns, `checks/phase-a` Endpoint/SendWith/limits examples and frankfurter policy excerpt: concrete generated send result, bounded provider document with metadata. Embedded native/openai/google-drive/fred/S3 references were read as guide content only; source files were not opened, and no streaming/multipart/signing implementation was used. |
| `docs/CHEATSHEET.md` | Embedded Docs declaration: generated options and path/header composition |
| `docs/RECIPES.md` | Embedded native/compat/app/sdk patterns and linked SDK discussions: explicit retries, clone semantics, opaque cursor projection, preserving native boundaries. Only prose/embedded excerpts were consulted, not linked files. |
| `examples/testserver/Cargo.toml` | Shared dev-dependency name/features |
| `examples/testserver/src/lib.rs` | TestServer, Recorded, Reply, Script, Gate, Body and shutdown APIs; used shared infrastructure without copying server code |

Observable author diagnostic: first test compilation used the nonexistent
`Resume::Next` variant (E0599) and lacked integration-test crate docs; reading
the public rustdoc corrected this to `Resume::At` and adding crate docs removed
the warnings. This was an author mistake, not a shared API failure. Build/test
results after correction are recorded separately in VERIFICATION.md.
