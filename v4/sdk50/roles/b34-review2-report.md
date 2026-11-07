1. [crates/concord-macros/src/emit.rs:1714](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/b34-core/crates/concord-macros/src/emit.rs:1714) — **CONFIRMED: source compatibility break for handwritten endpoint items named `operation`.** An SDK could previously supply the same convenience itself:
   ```rust
   impl StoreItem {
       pub fn operation() -> concord::Operation {
           <Self as Endpoint<Item>>::operation()
       }
   }
   ```
   The new generated function produces duplicate inherent definitions (`E0592`). A handwritten receiver method or associated constant with that name also conflicts. Separately, concrete `StoreItem::operation()` previously resolved through another in-scope trait can now silently select the inherent function. These follow from Rust’s lookup rules; no affected caller was found in the inspected corpus. **Smallest compatible fix:** retain the trait-only API by default and make inherent generation opt-in.

The following claims are sound:

- **Generated declarations do not collide internally.** A value/body named `operation` remains a field with `set_operation`/`maybe_operation` controls. Scopes and endpoints named `operation` generate navigation on their parent; the new function belongs to the endpoint type. Endpoints cannot contain children.
- **Generic and receiver lookup remain sound.** For a type parameter `E: Endpoint<T>`, `E::operation()` still selects the trait function. Explicit `<E as Endpoint<T>>::operation()` is unchanged. The new receiverless function does not participate in `value.operation()` lookup or displace an extension trait’s receiver method.
- **Generated identity is unchanged.** Both accessors use the same declared name and HTTP method as prepared-request metadata. `Operation` equality compares endpoint classification, name and method. This applies to typed, drained and streaming outputs. Square’s comparison therefore preserves its behavior.
- **No additional cfg or forwarding defect found.** The inherent impl and trait impl receive the endpoint’s effective cfg attributes together. The new function does not reference forwarded output types or expressions.
- **The timeout edits preserve every assertion.** Both clients disable proxies, redirects and request retries; the bound, non-listening socket remains alive through the assertion. The 30-second whole-request timeout remains finite, so the change introduces no unbounded connection wait.
- **The timeout-race explanation is sound.** Reqwest’s whole-request deadline can expire before Windows reports its connection failure; that deadline error lacks the connect-error chain required by `is_connect()`. Thirty seconds provides margin. “About two seconds” is an environment observation, not a portable Windows guarantee. Extreme delay or filtering can still fail these tests, but that vulnerability predates this change.

Areas not covered: no compilation, tests, socket experiments or external services were used. The cited Windows CI run and exact refusal timing were not independently verified. The new `b34_operation` test covers typed/unit accessor equality and one prepared request; it does not itself cover handwritten-item conflicts, trait lookup, cfg, macro forwarding, streaming, or received/error propagation.