I1-wire-display: complete SDK overlays against rv-p1 ba10189b; no Concord implementation or compilation.

Historical experiment: owner decision 7 (P4.1) replaces the scalar-display derive described here
with `serde_plain::derive_display_from_serialize!`. The implementation and hidden serde bridge
have been removed; see `docs/MIGRATION.md#sdk-owned-formatting` for the current contract.

Manual spelling glue removed: github 48 lines (35 net src lines); stripe 88 (71 net); frankfurter 0; classifier glue-category delta is 0 (REVIEW.md explains).
New concepts: one opt-in concord::WireDisplay derive; existing serde attributes supply its scalar text; no api! syntax or dependency changes.
C1: api! bodies are identical; serde spells provider values, with no generator controls.
C2: scalar spelling bridges disappear; validation, joins, formatting exceptions and behavior remain Rust.
C3: Frankfurter's complete src/lib.rs is byte-identical; its Date and strings need no wrapper or derive.
C4: explicit serde rename, rename_all, transparent and untagged rules; encoding still belongs to each placement.
C5: PARTIAL—non-scalar Serialize shapes fail at prepare() with Build/NotSent naming the placement, not at the derive site.
C6: explanation: derive Display from scalar serde spelling; keep manual Display when spellings differ.
C7: ordinary Display; existing reqwest clients, prepare(), into_parts() and send_raw() remain available.
C8: deterministic formatting only; no extra I/O, retries, actors or global state.
C9: proposed literal-text, wire, unsupported-shape and canary checks are supplied/referenced; unrun, so no tested guarantee is claimed.
Simple control changed: no SDK source change; only an ordinary caller example is supplied.
Existing tests needing change: none; added tests and the remaining compiler acceptance gates are listed in REVIEW.md.
Does the declaration read better than before? No: api! is unchanged; supporting value code shrinks, while compatibility keeps the as_str() macros.
