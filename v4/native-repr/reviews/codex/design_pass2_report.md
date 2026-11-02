**Verdict: ADOPT WITH CHANGES.** The combined representation design is coherent, but several contract details need correction before implementation. This is a static design verdict, not acceptance of an implementation.

Observed session settings: **`gpt-6.1-sol`, `xhigh`**, recorded in the current session’s `turn_context`; read-only sandbox, approval policy `never`.

The counterexamples below are static deductions. I did not compile the proposed syntax, which is absent from this snapshot.

1. **High — The proposed `Debug` selection can bypass runtime sensitivity for open-trait types.**

   Track H §5.3/§5.5 selects `DebugDisplay` whenever a value also participates in a text placement. That conflicts with N02’s promise to redact flagged header values in generated owner `Debug`.

   Consider a local `SecretHeader` that wraps a sensitive `HeaderValue`, implements `ToHeaderValue` by cloning it, and implements `Display` by rendering its text:

   ```rust
   client C(token: SecretHeader) {
       headers { "x-private" = token; }

       GET safe();
       GET search() {
           query { "q" = token; }
       }
   }
   ```

   Under the reported selection rule, the text use makes owner `Debug` render `token` through `Display`. Formatting the client can disclose it even when the application only uses `safe()`. No stateful or malicious conversion is necessary.

   **Required change:** Track header use independently from text use. Before displaying a value that participates in direct header conversion, honor the returned header’s sensitivity flag. Protection of its separate query/template uses can still require `#[sensitive]`; this does not require runtime propagation between sibling header values.

   The direct `HeaderValue` case avoids this particular route because it lacks `Display`. The gap concerns the explicitly open trait and types supporting both routes.

2. **Medium — Automatically inserted body content types need the header key’s static sensitivity classification.**

   This valid combination exposes an integration gap:

   ```rust
   client C(
       #[sensitive] header("content-type") ct?: HeaderValue
   ) {
       request crate::PrivateCodec;
       POST save(body p: Payload);
   }
   ```

   When `ct` is absent, the codec supplies `Encode::CONTENT_TYPE`. Existing sticky classification says `content-type` remains sensitive. However, the current [`Parts::finish`](/home/canarit/projects/concord_v4_native_work/snapshots/codex-pass2-93cf892/crates/concord/src/__private.rs:352) inserts the codec header outside the classified header-contribution path.

   Reusing that insertion unchanged would produce an unflagged fallback and reveal it in `Prepared::Debug`. A custom codec’s media type could itself contain confidential application information.

   **Required change:** Apply effective static classification to automatically inserted content types, including after removal or replacement of declared contributions. Preserve the distinction: a removed value’s *runtime* flag should not taint its replacement.

3. **Medium — Rows 8a–8c need precedence defined over the composed endpoint, plus actual runtime presence.**

   “Declared `content-type`” and “on one endpoint” are insufficiently precise:

   ```rust
   client C(header("content-type") ct?: HeaderValue) {
       request json("application/vnd.api+json");

       POST save(body p: Payload);
       POST plain(text p: String);

       scope clean {
           headers { remove "content-type"; }
           POST save(body p: Payload);
       }
   }
   ```

   My recommended interpretation is:

   - Root `save`: reject the potentially surviving header writer plus the inherited explicit media type.
   - `plain`: discard the request default’s format **and media type**; the optional header can override Text’s implicit media type.
   - `clean.save`: allow it because removal eliminates the inherited header writer; use the inherited body media type.

   State whether this is the intended contract. Conflict checking should inspect surviving contributions and conditional fallback branches, rather than every ancestor declaration or only endpoint-local syntax.

   For an **implicit** codec media type, pin these four cases explicitly:

   | Body present | Native header present | Resulting content type |
   |---|---|---|
   | No | No | None |
   | No | Yes | Native header |
   | Yes | No | Codec default |
   | Yes | Yes | Native header |

   An empty native value counts as present. An empty `each` collection contributes no value. Conversion failure must remain a build error, not become absence.

   Two associated omissions also need resolution:

   **Unused codec constants:** [`encode_body`](/home/canarit/projects/concord_v4_native_work/snapshots/codex-pass2-93cf892/crates/concord/src/__private.rs:151) currently validates `E::CONTENT_TYPE` before header precedence is applied. Specify whether an explicit body media type or surviving header bypasses validation of an unused malformed codec constant. The natural meaning of “replaces” favors bypassing it.

   **Media-type validation:** `HeaderValue::from_static("nonsense")` validates header bytes, not media-type syntax. Define whether explicit media strings validate `type/subtype` and parameters, or only header safety. Also specify the treatment of repeated `content-type` values; HTTP defines this as a singleton field. ([RFC 9110 §8.3](https://httpwg.org/specs/rfc9110.html#field.content-type))

   Rows 1/2b should say that the **representation default adds no content type**. Independently declared headers and the supplied reqwest client can still supply one.

4. **Medium — Rejecting inherited `-> ()` violates the alias requirement.**

   ```rust
   type Unit = ();

   // Inside a client:
   response json;
   GET a() -> ();
   GET b() -> Unit;
   ```

   Row 3b rejects `a`, while row 5 permits `b` and decodes JSON unit. Calling the rejection “only a diagnostic” does not resolve this: compilation acceptance is observable behavior.

   **Required change:** Accept inherited `-> ()` with exactly the same decoding behavior as its alias. No `->` remains the distinct drain operation. Explain the empty-204 hazard in documentation and diagnostics for genuine failures.

   The proposed advice to write `-> json ()` is also inappropriate as a universal fix when the inherited format is a custom unit decoder.

5. **Medium — The feature/cfg promises require a combined resolution strategy that neither report establishes.**

   ```rust
   response json;

   #[cfg(feature = "api-json")]
   GET a() -> Item;

   #[cfg(feature = "api-json")]
   GET b() -> Item;

   GET health() -> text String;
   GET ping();
   ```

   With both JSON features disabled, the unused default should not reference `Json` if row 12b means resolution occurs only for active consumers. Track RA explicitly reports that its eagerly emitted owner alias fails this case.

   Conversely, independently resolving the default at every endpoint can recreate multiple missing-feature errors, contrary to 12a’s “one compile error.”

   **Required change:** Define unused-default behavior and prototype it together with diagnostic deduplication. A shared alias gated by the union of its consumers’ effective cfg conditions is one viable stable-Rust approach; include ancestor conditions and exclude endpoints that override or never consume the default.

   This is implementable. The issue is that the combined contract currently promises more than the separate prototype evidence establishes.

6. **Medium — Row 9 overpromises control over trait diagnostics.**

   ```rust
   response json;
   GET image() -> concord::bytes::Bytes;
   ```

   The existing JSON implementation is conditional on `DeserializeOwned`. Failure can therefore surface as a serde bound error rather than Concord’s `Decode` message. Track RA §6 E3 already reports this exact limitation.

   `on_unimplemented` is a compiler hint, and its substitutions identify trait types and generic arguments; they do not provide arbitrary declaration-origin information or guarantee the original DSL spelling. ([Rust Reference](https://doc.rust-lang.org/reference/attributes/diagnostics.html#the-diagnosticon_unimplemented-attribute))

   **Required change:** Promise useful spans and actionable trait context, with custom messages where applicable. Do not require every failure to produce Concord’s wording, the word `bytes` rather than `Binary`, or exactly one compiler diagnostic.

7. **Low — The rationale for omitting default clearing misstates inheritance.**

   N06 says a mixed subtree can “declare no default.” Under a client default, silence still inherits:

   ```rust
   response json;

   scope heterogeneous {
       GET version() -> String;
   }
   ```

   This remains JSON, and the scope cannot demand local format selection.

   Clearing need not be added without a demonstrated use case. Correct the guidance instead: put defaults on uniform child scopes, omit the client default, or explicitly annotate every exception. An empty scope declaration does not clear anything.

The primary syntax itself earns adoption. Owner defaults and local format declarations serve different purposes: owners establish policy for descendants; endpoint lines select exceptions. `body item: T` expresses inheritance, while `json item: T` fixes the format locally—even when it currently matches the default. That is meaningful redundancy, not two synonymous primary DSLs.

Pure RA reduces repetition but hides exceptions in endpoint blocks. Pure RB maximizes local readability but retains cross-endpoint repetition. Neither strictly dominates the combination. Removing endpoint `request`/`response` statements is the right constraint.

I found no fundamental parser ambiguity requiring a different design. The grammar should nevertheless pin these cases:

| Declaration fragment | Required interpretation |
|---|---|
| `json: T`, `json?: T` | Ordinary value named `json` |
| `header("x") json: T` | Header value named `json` |
| `json query: T` | Body named `query` |
| `self::csv rows: T` | Custom format |
| `json::Codec rows: T` | Custom path, not built-in `json` |
| `-> Vec<Item> { … }` | Inherited format |
| `-> codecs::Csv Vec<Item>` | Explicit custom format |
| `-> codecs::Csv ()` | Explicit custom format with unit payload |

Also decide `-> json { … }` when a lowercase payload alias named `json` exists. The `-> T` rule suggests inheritance; keyword-first parsing suggests a missing payload. Define raw-identifier behavior consistently. These are lexical precedence decisions, not reasons to reject the design.

The native-header boundary is otherwise well justified. Concrete open-trait implementations plus templates avoid the reported coherence failure and retain ordinary caller types. Adding `IntoValue<HeaderValue>` for `&HeaderValue` is compatible with the exact-type conversion rule. Cloning native values, preserving their flags, and adding static classification per value is sound in principle. Keep the documented template migration for foreign `Display` types.

Calling `to_header_value()` from `Debug` can allocate, panic, or execute user code. Document repeated invocation and conversion obligations; that risk alone does not justify another wrapper framework. The per-value sensitivity rule is reasonable: an independently unflagged appended sibling need not become sensitive.

The remaining truth-table choices are sensible: absent bodies versus JSON null and empty bodies; no-arrow draining; independent directions; custom marker codecs; no inferred negotiation; native multipart/streaming exits; and retained status/decoder behavior. They still need combined implementation evidence.

**Evidence personally executed:** Git identity/status checks; narrow inspection of the current session metadata; read-only source searches; and Git-blob/mode verification of all **460** manifest-listed files, with no mismatches. Snapshot HEAD is `58f73a78d382c168af83b063e5b875fe54a1c8aa`; the manifest records original source SHA `93cf89296c4942778a93c1075cb26dcfce856045`. Git remained clean and has no remote.

**Inspected only:** The supplied charter, decisions, contract and specialist reports; relevant parser, composition, conversion, body and decoder source; and dependency source/documentation. Specialist test counts, MSRV runs, mutations, performance measurements and prototype claims remain their reported evidence. I ran no Rust compilation, tests, benchmarks, server probes or rendering, and made no writes, pushes, publications or settings changes.