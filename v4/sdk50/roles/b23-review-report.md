Reviewed `273798be` and `53096208`, including cached sources for locked reqwest **0.13.5**, hyper **1.12.0**, and hyper-util **0.1.21**. No builds, test execution, changes, or external services.

1. **P1 — `NotSent` is unsafe evidence for the whole call. CONFIRMED by reading.** [error.rs:351](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/integration/crates/concord-core/src/error.rs:351)

   A POST can commit, return `303 Location: …`, and then fail to connect to the redirect target. Reqwest follows redirects by default and returns the final connection error. Concord retains no head and returns **`NotSent`**, although the POST reached the server and completed. Disabling retries, as the test does, leaves redirects enabled.

   Conversely, exhausting the redirect limit returns **`Unknown`** despite receiving redirect response heads.

   There is another unsafe path: reqwest’s `is_connect()` searches the **entire error source chain**. A streaming upload can send data, then fail with a nested reqwest connection error—for example while fetching its next chunk—and make the outer error satisfy `is_connect()`.

   **Smallest safe fix:** classify headless transport failures as `Unknown` and define `Answered` around a response exposed to Concord. Stronger delivery guarantees require tracking redirects/attempts and error provenance.

2. **P2 — substitution rewrites underscores that are not output placeholders. CONFIRMED by reading.** [emit.rs:1752](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/integration/crates/concord-macros/src/emit.rs:1752)

   `with_output` replaces every underscore identifier, including the underscore in a lifetime. For an `Item` endpoint, `send P -> fn(&'_ _);` should produce `fn(&'_ Item)`; it also rewrites `'_`, corrupting the type. Wildcard patterns inside const expressions are likewise rewritten.

   **Smallest fix:** replace `syn::Type::Infer` nodes through an AST traversal, preserving lifetime and expression underscores. Handle placeholders inside opaque type-macro inputs explicitly.

3. **P2 — forwarded templates bypass existing fragment-preservation machinery. PLAUSIBLE failure scenario.** [syntax.rs:1020](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/integration/crates/concord-macros/src/syntax.rs:1020), [emit.rs:1755](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/integration/crates/concord-macros/src/emit.rs:1755)

   The template uses raw `Type` parsing instead of the repository’s `parse_type`, and substitution reconstructs every group, including unchanged opaque fragments. Both protections already exist elsewhere for forwarding bugs.

   A wrapper declaring `send P -> Envelope<_, { $e * 2 }>;`, invoked with `$e = 1 + 2`, can consequently select `Envelope<T, 5>` instead of `Envelope<T, 6>`. Rebuilding fragments inside a type-macro invocation presents the related hazard documented by `located`.

   **Smallest fix:** use `parse_type` for the template and preserve unchanged opaque fragments during substitution. The exact compiler consequence remains unexecuted under this review’s constraints.

4. **P2 — the mismatch check permits unequal policy outputs through coercion. CONFIRMED by reading.** [emit.rs:1634](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/integration/crates/concord-macros/src/emit.rs:1634)

   A typed assignment checks coercibility, not type equality. If `P::Output` is `Box<[T; 1]>`, declaring `send P -> Box<[_]>;` accepts the unsizing coercion. Generated `send()` and `Sends` then expose `Box<[T]>`, although that is not the policy’s associated output. This contradicts AUTHOR_GUIDE §5.13’s unconditional mismatch claim.

   **Smallest fix:** enforce an associated-type equality bound, `SendWith<T, Output = Declared>`, with the diagnostic located at the template.

5. **P3 — generated query-type documentation can name a different type. CONFIRMED by reading.** [emit.rs:1876](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/integration/crates/concord-macros/src/emit.rs:1876)

   Removing every space also changes literal contents and merges required token boundaries. For example, `Options<{ "a b".len() }>` is documented using `"ab"`, changing its const argument. Types containing `dyn Trait` also lose meaningful spacing.

   **Smallest fix:** use the existing `type_text` formatter.

6. **P3 — the new guide contains inaccurate error-handling examples. CONFIRMED by reading.** [AUTHOR_GUIDE.md:1618](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/integration/docs/AUTHOR_GUIDE.md:1618)

   The `uncertain` function’s comment includes writes without a readable answer, but a committed POST returning malformed JSON with status 201 produces `Answered` plus `Decode`; the function returns `false`. The smallest fix is to narrow the comment to the implemented SDK rule, or explicitly include the desired unreadable-response cases.

   The delivery table also overstates connection knowledge: a total timeout during DNS/connection establishment returns `Unknown` without establishing a connection. Describe `Unknown` as insufficient delivery evidence.

   In §5.11, [line 1560](/home/canarit/projects/Concord_SDK50_Ecosystem_Handoff/work/integration/docs/AUTHOR_GUIDE.md:1560) lists `error.head()` among `HeaderMap` inputs; it returns `Option<ResponseHead>`. The example needs unwrapping and `.headers()`.

The following claims hold by inspection:

- **Field semantics are unchanged.** `single` and `unsigned` are direct extractions, including repetition precedence, trimming, digit validation, overflow handling, borrowing, and value-free errors. Both `FieldError` export paths name the same type.
- **The delivery match implements the stated mapping.** Build/header conflict/encoding failures occur before Concord sends; retained response heads correctly take precedence over transport classification. Status, metadata, decode, and limit errors classify as answered.
- **Ordinary connector failures precede endpoint transmission.** Pool checkout and connector acquisition happen before request dispatch. TLS handshake failures, connector timeouts, and failed proxy tunnel negotiation can occur after TCP establishment or negotiation bytes, while still preceding the endpoint HTTP request. Thus “never connected” and “no request byte” need careful qualification. I found no separate pool/HTTP/2 dispatch defect beyond the history/provenance problems above.
- **The common declared-result path works structurally.** Nested type placeholders are substituted; the declared result appears in both signatures; drain output is `()`, untyped stream output is `Incoming`, and typed stream output is `Records<F, T>`. Value-policy cloning and qualified policy-type emission remain intact. Effective cfg attributes cover the new `Sends` implementation. `located_at` preserves token hygiene, and the supplied `Option<_>` mismatch snapshot points at the template.
- **The ordinary §5.13 example and corresponding cheatsheet row match generated signatures**, subject to the equality and forwarding findings above. The field-reader cheatsheet row matches implementation.

Coverage remains limited:

- `w2_delivery.rs` omits redirects, retry history, nested upload errors, pooling, HTTP/2, proxies, TLS, separate connect/total timeouts, and delivery assertions for encoding, header conflicts, and limits.
- The declared-result tests omit value policies with templates, qualified policy types, declared stream results, cfg combinations, multiple/nested placeholders, lifetime/pattern underscores, forwarding, and coercible mismatches.
- `_names_the_result` verifies type compatibility; it cannot establish that rustdoc spells the concrete result instead of an equivalent associated-type projection. No generated-token or rendered-doc assertion covers that claim.