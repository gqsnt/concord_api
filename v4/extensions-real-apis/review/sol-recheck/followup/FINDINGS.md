# SOL independent follow-up — 66fde2ac

**Verdict: ACCEPT. All items in this follow-up scope are RESOLVED. No new finding.** Shopify's resolution is explicitly documentation of page replay; exact resumption remains outside the application's API.

Reviewed detached `66fde2acfceccf3a2a74de877062837f2dfa00f8`, comprising `b528bc79`, `fc166290` and `66fde2ac` above the prior `d2681065` review. I inspected the changes and ran independent loopback probes adapted from the prior review, plus new stored-text and literal-fragment cases. No live provider calls or sub-agents. No tracked files were modified. All new material is under `review/sol-recheck/followup/`; previous evidence is preserved. The final independent probe lockfile uses repository-lock dependency versions; its only additional package identity is `sol-recheck-followup`.

| Item | Status | Source and observed reproduction |
|---|---|---|
| OPUS-1 numeric-link cross-repository restore | **RESOLVED** | `examples/github-sdk/src/paging.rs:189` persists the binding; `:358`–`:368` checks repository, operation and parent before granting a key. A provider-returned numeric link from A restores identically through A, while B gets `ContinuationMismatch` identifying A and B. Refusal sends no request. |
| SOL-R3 rename round trip | **RESOLVED** | `paging.rs:376`–`:382` checks operation/parent path without overriding the persisted repository identity. A real loopback 301 from old-name to new-name, followed by a named continuation, now round-trips through the original old-name factory and fetches the named page successfully. |
| SOL-R1 fragment-only progress | **RESOLVED** | `paging.rs:498` and `:525`–`:535` strip fragments before comparison and from stored NextLinks. Changing fragments fails on request 2, rather than reaching the budget. Literal `<#v1>`, `<#>` and `<>` self-links fail on the first request. |
| SOL-R2 Kubernetes template token | **RESOLVED** | `examples/kubernetes-sdk/src/list.rs:281`–`:294` sets the request token from the supplied key, including None. Observed token sequence `[None, seed-A, next-B, None]`; repeated next-B is refused on request 3. Explicit resume overrides a different template token and retains snapshot checking. |
| OPUS-6 replay documentation | **RESOLVED, documentation scope** | `examples/shopify-app/src/catalog_inventory.rs:228`–`:231` and README:47–:49 correctly describe replay of earlier parents and caller discard/deduplication. All 12 products/collections × first/final page × first/middle/last failure cases still report the correct page and repeat 0/1/2 successful siblings when replayed. No resume API or exact-once implementation is claimed. |
| Stored representation/parser checks | **RESOLVED; no material parsing defect reproduced** | `paging.rs:202`–`:232`, `:350`–`:382`; 93-case matrix plus operation/parent boundary tests passed. Accepted keys have stable canonical round trips; malformed, mismatched and foreign cases are refused without I/O. Details below. |

**Reproduction commands and results.** From the repository root:

```sh
export CARGO_TARGET_DIR="$PWD/review/sol-recheck/followup/target"
export TMPDIR="$PWD/review/sol-recheck/followup/tmp"

cargo test --offline -p concord-example-github-sdk -p concord-example-github-app -p concord-check-maintenance-github -p concord-example-kubernetes-sdk -p concord-example-kubernetes-app -p concord-check-maintenance-kubernetes -p concord-example-shopify-app

cargo test --offline --locked --manifest-path review/sol-recheck/followup/probes/Cargo.toml --tests -- --nocapture
```

The requested regression suite passed **159 tests/doc-tests, zero failed, zero ignored**: `logs/regressions.log`. The final independent suite passed **12 tests, zero failed, zero ignored**: `logs/independent-final.log`. Those tests contain the 93-case parser matrix, additional operation/parent checks, 32 adapted URL-path cases and 12 Shopify failure/replay scenarios. All final assertions check the repaired behavior. Unlike the previous review's residual-behavior probes, they do not treat reproducing an old defect as success. `COMMANDS.sh` records the commands above.

**OPUS-1 and SOL-R3 evidence.** `probes/tests/github.rs:83` obtains the continuation from an actual first response for `org/repo-a`, rather than constructing a fake key. The serialized text is asserted exactly:

```text
github-link/1 issues/list-comments org/repo-a 11 http://<loopback>/repositories/101/issues/11/comments?page=2
```

The test checks both the in-memory mismatch and persisted mismatch through B, verifies expected/found bindings and unchanged request count on refusal, restores through A, and records the successful page-2 HTTP request. Observed output:

```text
OPUS-1: A numeric link round-trips through A; B rejects stored A binding; no request on mismatch
```

`probes/tests/github.rs:110` uses an HTTP 301, not just a manually changed path. The old-name request redirects to new-name; the response returns new-name's next link. Restoring `key.to_stored()` through the original factory preserves equality with the in-memory key and permits the continuation fetch. Observed:

```text
SOL-R3: same-origin rename redirect; provider new-name Link round-trips through old-name factory and fetches
```

Repository identity now comes from the stored binding. A matching stored binding may therefore accompany another named path or numeric ID; the tests deliberately accept such URL aliases. A different *stored* repository is refused in either URL form. This is the intended change needed for numeric links and renames, not the previous behavior of giving a raw URL whichever binding the restoring factory happens to have. The representation does not authenticate manually edited binding/URL pairs, and these tests do not claim it verifies their provenance. Raw URLs alone now return `MalformedStoredLink`; callers must persist `to_stored()` as documented at `paging.rs:177` and `:183`.

**SOL-R1 evidence.** `probes/tests/github.rs:154` reuses the prior trick of returning a fresh fragment on every response. Previously this made five successful calls until the budget. Now only the first page succeeds; request 2 produces `LinkDidNotAdvance`, the chain becomes `Failed`, and no fragment appears in the returned key or its stored form. Observed:

```text
SOL-R1: changing fragments removed, error at request 2, Outcome::Failed:
["/repos/org/repo/issues/11/comments", "/repositories/101/issues/11/comments?page=2"]
```

`probes/tests/github.rs:178` adds fragment-only relative references and a restored link containing `#stored`. Literal `#v1`, `#` and empty references fail on the initial request. Both `#new` and `?page=2&cursor=a%23b#new` fail on the first resumed request. The restored fragment is absent, the escaped `%23` remains in the cursor and actual wire target, and the protocol error retains response metadata. `probes/tests/github.rs:134` also confirms Start → page 2 → page 3 → page 3 still fails on request 3, against the current URL.

**SOL-R2 evidence.** `probes/tests/kubernetes.rs:8` sets the template's continuation to seed-A and its resourceVersion to 77. The first response deliberately returns seed-A: that response is valid because no token was requested. The next request sends seed-A and returns next-B; the next sends next-B and is refused when next-B repeats. Calling `fetch(None)` again clears the token again. The first request retains resourceVersion/resourceVersionMatch; continued requests omit both, and the configured limit is preserved.

```text
SOL-R2: sent [None, seed-A, next-B, None]; repeat rejected at request 3;
first RV preserved and continued RV cleared
```

`probes/tests/kubernetes.rs:44` supplies an explicit key-C alongside an unrelated template-unused token. The server asserts key-C was sent and no resourceVersion parameter survived. A changed response snapshot returns `InconsistentSnapshot`, preserving the previous consistency check (`list.rs:314`).

**Stored-form parsing evidence.** `probes/tests/stored.rs:21` runs **93 cases**, producing:

```text
stored parser: 93 cases; accept/malformed/mismatch/foreign=[18, 33, 32, 10];
every accepted key has stable canonical round-trip
```

The checks cover:

- Strict version tag and literal-space framing: leading/trailing/repeated spaces, missing/extra fields, tab/newline/NBSP separators and a BOM are rejected. A literal space inside the URL creates an extra field and is refused; `%20` in the query is accepted.
- Repository binding: ASCII owner/name case variants are accepted and normalized to the factory's binding. Other repositories, percent-encoded binding names and Unicode lookalikes are refused. Empty owner/name, missing slash and extra slash are malformed.
- Parent forms: child 11 accepts canonical `11`; `-`, 0 and other canonical values mismatch that factory. Leading zeroes, signs, decimal/exponent/hex forms, Unicode digits, whitespace and overflow are malformed. Additional tests at `stored.rs:138` round-trip `-` for top-level listings and canonical 0/u64::MAX through matching typed factories; this tests representational boundaries, not live GitHub validity of issue zero.
- Operations: `stored.rs:164` checks all **13 declared listing IDs**. The expected ID succeeds; other known IDs mismatch and preserve their correct diagnostic ID. Unknown, empty, case-changed and newline-suffixed operation IDs mismatch with `UNRECOGNIZED`. No request is made.
- URL collection shape: another parent/operation, extra suffixes, trailing slash, zero-padded/encoded parent, encoded operation, operation-path case changes and encoded slash tricks are refused. URL-normalized dot segments are accepted only when the resulting collection matches. Foreign host/scheme/port/base and misleading userinfo-host constructions are refused after URL parsing. Query/fragment text cannot disguise a wrong operation/parent.
- Query preservation and URL normalization: accepted cursor percent encodings remain intact, while fragments are removed. Host case is normalized by Url. Same-origin userinfo is accepted, as permitted by the existing origin check; apparent trusted text before `@evil.test` does not bypass it. The URL token also accepts a trailing LF or embedded tab and normalizes it using `Url::parse` (`paging.rs:231`); this is permissive URL normalization, not strict canonical-text validation. The resulting key still passes origin/collection checks and serializes back into a stable canonical form. No binding/origin bypass was reproduced from those cases.

Every matrix acceptance is re-serialized, restored and compared for key equality and canonical text stability. Every rejection is checked for the expected typed category, no response metadata and no URL-sentinel exposure through Display/Debug. The server records zero requests during all restore-only matrices. Canonical `to_stored()` output need not contain these noncanonical spellings; accepting some equivalent Url spellings does not make a different repository binding, operation, parent or origin pass.

**Shopify documentation evidence.** The adapted `probes/tests/shopify.rs:37` re-runs the original two-page, three-parent matrix. Each result is `Stopped` at the interrupted page, including final-page failures. Later siblings remain available on replay. First/middle/last parent failures repeat respectively zero/one/two completed siblings, plus the failed parent. Representative output:

```text
products page=2 failed_item=3: Stopped, correct page; replay repeats 2 successful siblings and failed parent; remaining siblings included
collections page=2 failed_item=2: Stopped, correct page; replay repeats 1 successful siblings and failed parent; remaining siblings included
```

This agrees with the new README and `TraversalSummary::resume` documentation: the checkpoint is a page location, and a caller retains responsibility for discarding/deduplicating the replayed prefix. `ReconcileRequest` still has no resume input. The original wrong-page reporting problem is fixed, and the follow-up request to document the replay limitation is satisfied; no exact-once feature is implied by this RESOLVED status.

**New findings: none.** No additional correctness defect, compatibility risk, performance-resource limit, documentation gap or developer friction was reproduced in this follow-up scope. The requested consumers all pass. Final tracked-file checks (`git diff --exit-code`, `git diff --cached --exit-code`, and `git status --porcelain --untracked-files=no`) are clean. Evidence and executable probe sources remain under `followup/`; the earlier report is intentionally unchanged historical evidence.
