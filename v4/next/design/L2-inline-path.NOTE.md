L2-inline-path: additive `at "…";` suffix; L1 keeps short existing blocks on one line.
Glue removed, hand-counted from the full file diffs: kubernetes 0; mediawiki-action-api 0; nager-holidays 0.
Declaration delta: Kubernetes 21 replacements, −126 characters, 0 lines; MediaWiki 0; Nager 0.
New concepts: one optional endpoint spelling, `METHOD name(…) [-> T] at "relative/path";`.
C1: `at` states the provider's literal path; no Concord configuration facts added.
C2: path-only sugar; all error, context, paging and response glue remains in Rust unchanged.
C3: Nager's declaration, modules and existing tests are byte-for-byte unchanged; only a caller example is added.
C4: exact lowering to `{ path "…"; }`; no path inferred from an operation name or HTTP method.
C5: duplicate paths fail at the second path; existing template diagnostics retain authored spans (acceptance fixtures).
C6: one-sentence rule: use `at` for a path-only body, otherwise use the existing block; two spellings are the cost.
C7: reqwest ownership, prepare(), into_parts() and send_raw() keep their existing semantics.
C8: syntax only; no additional execution, retries, state or actors.
C9: existing tests unchanged; new pass/compile-fail specimens in acceptance/; none compiled or run, as requested.
Tests needing changes: none in any SDK; see DESIGN.md for coverage and the unchanged-module inventory.
Does the declaration read better? Yes: Kubernetes routes lose six characters each; the gain is small and saves no lines here.
