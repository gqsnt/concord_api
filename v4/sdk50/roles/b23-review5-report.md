Two defects remain.

1. **P2 — Renaming `_` changes parsing and placeholder classification. CONFIRMED by reading.** [crates/concord-macros/src/emit.rs:1831](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/integration/crates/concord-macros/src/emit.rs:1831)

   This valid type macro demonstrates a regression:

   ```rust
   macro_rules! result {
       (_::Tag, _) => { Result<Item, Error> };
   }
   // Declaration:
   send Policy -> result!(_::Tag, _);
   ```

   The original input fails the comma-separated-types parse, so the documented rule requires leaving it untouched. Renaming produces `marker0::Tag, marker1`, which parses successfully. The finder selects marker 1, and the replacement walk emits `result!(_::Tag, Item)`, breaking the literal matcher.

   Renaming also hides a supported forwarded placeholder. Forwarding `_` as `$t:ty` into `result!($t::Assoc)` supplies `Group(None, _)::Assoc`. Syn’s original parse creates a qualified-self type containing `Type::Infer`. After renaming, Syn instead merges the group into the multi-segment path `marker0::Assoc`; `get_ident()` fails, so the placeholder is missed.

   **Smallest complete fix:** classify `Type::Infer` occurrences using the original parse and map them to original token positions. Apply that rule recursively at macro boundaries. Checking original parse success alone fixes the first scenario, but not the forwarded-fragment miss.

2. **P2 — Existing identifiers can impersonate generated markers and fill non-type underscores. CONFIRMED by reading.** [crates/concord-macros/src/emit.rs:1807](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/integration/crates/concord-macros/src/emit.rs:1807)

   Concrete failure:

   ```rust
   macro_rules! result {
       (__concord_placeholder_0, [u8; _]) => { Result<Item, Error> };
   }
   // Declaration:
   send Policy -> result!(__concord_placeholder_0, [u8; _]);
   ```

   The existing first identifier inserts index 0 into `fill`. The actual underscore 0 is an array-length expression, which the visitor correctly skips—but the replacement walk nevertheless replaces it with `Item`. The macro matcher then fails.

   **Smallest fix:** choose marker names absent from every original identifier, including nested inputs. Merely checking that the index is in range does not prevent this collision.

Claims found sound:

- Both walks number non-lifetime underscores identically: the same depth-first traversal includes expression and parameter-name underscores. Nested macro analysis uses those existing numbers.
- The ordinary earlier cases remain correct: `fn(_: u8) -> _`, `[_; { let _ = 1; 2 }]`, `result!{_}`, and nested type macros.
- Original punctuation tokens retain their spacing, addressing the reported `&&` defect. The added literal-matcher fixture targets that failure.
- Unchanged forwarded groups are reused. Unchanged leaf tokens retain their spans; rebuilt groups retain their delimiter kind and combined `g.span()`.

Areas not covered:

- No builds, tests, external services, or file changes.
- No exhaustive review of arbitrary/verbatim syntax or rustc’s opaque-fragment behavior after rebuilding changed invisible groups.
- Separate opening/closing delimiter-span fidelity is not established by copying the combined group span.
- The added tests do not cover either finding above.