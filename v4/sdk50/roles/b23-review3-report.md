Three issues remain. Reviewed commit `20517134` and the prior findings using local code, tests, docs, and cached dependency sources. No builds, tests, changes, or external services.

1. **P1 — Redirected HTTP refusals can still conceal a performed write. CONFIRMED by reading; retained behavior.**  
   [Discord limiter.rs:375](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/integration/examples/discord-rest-sdk/src/limiter.rs:375), [Zendesk policy.rs:127](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/integration/examples/zendesk-support-sdk/src/policy.rs:127).

   A server performs a POST, returns `303`, and the redirect target returns `429`. Discord returns `RateLimited`, described as “refused” in its README; Zendesk returns `Http`, whose policy documentation says the request was not processed. Neither reports an uncertain write. The final response describes the redirected GET, so it cannot establish that the original POST was refused.

   The transport-error fix is correct, but it does not resolve this adjacent response path. Discord’s construction example enables redirects; Zendesk’s README instructs caller clients to disable retries without also requiring redirects disabled.

   **Smallest fix:** consistently require and demonstrate disabled redirects for definitive refusal guarantees, including caller clients. If redirect-following clients remain supported, preserve enough history to classify these outcomes conservatively. Add a performed-POST → `303` → `429` witness.

2. **P1 — §5.11b’s connect-rule conditions omit retry history. CONFIRMED by reading.**  
   [AUTHOR_GUIDE.md:1609](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/integration/docs/AUTHOR_GUIDE.md:1609).

   Owning the client, disabling redirects, and using fixed bytes are insufficient. The locked reqwest supports response-based retry classifiers and can clone buffered requests. An SDK-built client can retry a `503`: the first write commits and returns `503` with the connection closed; the next connection is refused. The returned error satisfies `is_connect()`, despite the earlier performed write.

   **Smallest fix:** require `.retry(reqwest::retry::never())` too, or require reliable evidence that every preceding internal attempt could not have performed the write. Client ownership alone is not that evidence.

3. **P2 — Type macros reintroduce replacement of parameter names and expression underscores. CONFIRMED by reading.**  
   [emit.rs:1769](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/integration/crates/concord-macros/src/emit.rs:1769), [emit.rs:1798](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/integration/crates/concord-macros/src/emit.rs:1798).

   The structural guarantee stops at `Type::Macro`. For an identity macro:

   ```rust
   macro_rules! identity { ($t:ty) => { $t }; }
   ```

   `send P -> identity!(fn(_: u8) -> _);` still rewrites both underscores. With endpoint output `Vec<u8>`, its effective input becomes `fn(Vec<u8>: u8) -> Vec<u8>`, which is invalid.

   Likewise, `identity!([_; { let _ = 1; 2 }])` substitutes the wildcard pattern inside the expression. Nested macro inputs receive the same unrestricted scan. Supporting every delimiter fixes `result!{_}`, but also exposes brace-contained expressions to replacement again.

   **Smallest fix:** apply structural substitution to supported type-shaped macro arguments. For arbitrary opaque macro grammars, define an explicit placeholder convention or restriction instead of claiming type-only substitution. Add wrapped versions of the parameter-name and wildcard-expression cases.

The following claims are sound by inspection:

- **Ordinary type traversal:** the locked syn visitor reaches qualified self types such as `<_ as Trait>::X`, generic arguments in trait bounds, `dyn Fn(_) -> _`, associated type bindings and constraints, parentheses, and invisible `TypeGroup` nodes—including `_` forwarded as a `ty` fragment. Lifetimes and function parameter names remain unchanged outside opaque macro inputs.
- **Expression exclusion:** array lengths and const expressions are skipped. This also deliberately leaves type syntax nested inside those expressions untouched.
- **Spans, hygiene, and forwarding:** I found no concrete new regression from re-emitting the parsed type. Identifier spans survive quoting; `TypeGroup` retains invisible delimiters; unchanged opaque macro groups are reused; `located()` preserves hygiene through `located_at`. The new `Envelope<…, 6>` fixture meaningfully checks forwarded-expression precedence. String comparisons alone cannot verify these properties.
- **UI cases:** draining produces `()`, independently of codec features. `Result<(), Error>` still mismatches `Option<()>`; `Box<[(); 1]>` still coerces to, but is not equal to, `Box<[()]>`. The associated-type equality bounds therefore continue testing the intended defects in every feature lane listed by the gate.
- **SDK transport classification:** after successful preparation, every transport failure on the declared writes now becomes uncertain. Preparation failures remain before dispatch. Discord GET/HEAD and Zendesk GET paths retain their ordinary read errors; I found no read newly classified as uncertain.
- **Rewritten refusal tests:** Discord requires `AmbiguousWrite`, `accepted == None`, and a connect error. Zendesk requires `Uncertain`, the original idempotency key, `NoResponse`, and a connect error. These assert the stricter contract.
- **§5.11:** the field reader now receives `head.headers()` inside the closure. The expression returns `Option<Result<Option<u64>, FieldError>>`; it no longer passes an `Option` as a `HeaderMap`. I found no remaining type error in that example or the executable §5.11b example.

Areas not covered:

- No compilation, test execution, rustdoc compilation, or feature-gate rerun.
- The SDK refusal tests exercise direct failures, not a performed write followed by a failing redirect.
- No compiled witnesses cover forwarded `_`, the broader qualified/bound/function-type matrix, nested macro grammars, or span/hygiene-sensitive opaque matching. Unstable and syn `Verbatim` type forms were not exhaustively assessed.