# ASTRA — Concord 4.4 independent review

Reviewed commit: `0dd655018892a8cae21479f1cd249d2c875f6c83`. “4.4” is the review label; this report does not recommend a package version. Scope: core/native response boundaries and the stream, CSV, and JSONL extensions. All changes made for this review are under `review/astra/`; no provider traffic, commits, pushes, or other worktrees.

**Judgment:** the existing core seams support these integrations, and the incremental readers do stream. Fix the CSV escape data loss and the resource issues below before treating the extension implementation as complete. Repeated metadata/error conversion warrants a small additive core convenience API in the next release, not a replacement of `Decode`, a universal endpoint abstraction, or a new streaming DSL.

| ID | Finding | Classification | Severity | Evidence | Disposition |
|---|---|---|---|---|---|
| ASTRA-1 | Accepted CSV escape dialect loses literal escape bytes on round-trip | correctness defect | high | reproduced | fix now |
| ASTRA-2 | CSV writer enforces size limits after allocating/copying the oversized record | performance-resource limit | medium | reproduced | fix now |
| ASTRA-3 | Empty chunks bypass cooperation in every incremental reader | performance-resource limit | medium | reproduced | fix now |
| ASTRA-4 | JSON readers process large unfinished chunks beyond the documented poll budget | performance-resource limit | medium | reproduced | fix now |
| ASTRA-5 | Preserving metadata requires repeated native send/error-conversion scaffolding | developer friction | medium | reproduced; cross-checked in sources | propose for next release |

Counts: **5 substantive findings: 1 correctness defect (high), 3 performance-resource limits (medium), 1 developer friction (medium)**. No separate compatibility-risk, documentation-gap, or speculative-opportunity finding. Documentation consequences and compatibility costs are included with the responsible finding.

## Caller task completed before implementation/evidence review

See `TASK_NOTES.md`, `task/src/lib.rs`, `task/tests/caller.rs`, and `logs/caller-pass.log`. The scratch crate has its own `[workspace]` and local path dependencies. Public guides/source docs and the GitHub SDK's public API were sufficient; no conclusions files were opened until all three tests passed and the task record was written.

- **Metadata:** declared GitHub `repo(...).labels()` returned its typed `Vec<Label>` beside `200 OK` and `x-github-request-id=astra-7`. The wrapper used `into_parts`, one execute, and the endpoint's decoder. The initial fixture omitted required `Label.default`; fixing the fixture resolved the only task failure. There were no caller-task compiler errors.
- **Upload:** declared `POST upload(body rows: StreamRows<Source>) { request Csv; }`. The instrumented source was `Send + !Sync + !Unpin`. Preparation performed **0** source polls and the native request was not cloneable. Abort after body delivery began: **4** rows, **16** source polls, one source drop, no later source polling; the slow receiver saw **32,826** wire-body bytes, EOF, and no final HTTP chunk. Counts are observations, not scheduling guarantees.
- **Runtime contradiction:** `CsvReader::new(CsvOptions::new(HeaderMode::Absent, b','))` rejected `text/csv; header=present` synchronously with `OpenErrorKind::Metadata` while the server withheld all body bytes. Status and a header were inspectable; `into_response()` subsequently yielded the entire original body. No guessing or silent header-policy change occurred.

The review probes below intentionally assert the **observed defect**, so a passing probe is a reproduction, not a claim that the desired invariant holds. Their comments make this explicit.

## ASTRA-1 — Escape dialects silently lose literal escape bytes

**Affected task:** write CSV under an advertised escape dialect, then read the result with the same options or send it to a peer using those options. Both finite rows and lazy uploads share the encoder.

**Locations:** `crates/concord-extras/src/csv/options.rs:270` documents escape semantics; `options.rs:460` passes the configured escape to `csv-core`'s reader; `options.rs:477` configures the `csv` writer; `crates/concord-extras/src/csv/write.rs:406` delegates row serialization. Independent consumer: `review/astra/task/tests/probes.rs:19`.

**Reproduction:** inside `review/astra/task`:

```sh
CARGO_TARGET_DIR=../target cargo test --offline --test probes escape_profiles_round_trip_probe -- --nocapture
```

Use `HeaderMode::Absent`, comma, `escape(Some(b'\\'))`, and the one-field tuple `(r"back\slash,comma",)`. Test both `double_quote(true)` and `double_quote(false)`. Both writers succeed and emit the quoted field containing one literal backslash. Both readers succeed but return **`backslash,comma`**, losing the backslash. The default dialect with no escape is a successful preservation control. Expected: the original field, or an explicit refusal if the selected writer cannot represent it. A successful, different value is unacceptable. Full output: `logs/probes-final.log`.

**Cause/recurrence:** the reader recognizes escapes within quoted fields, while the configured writer does not protect literal escape bytes. This is inherited behavior at the `csv`/`csv-core` integration boundary, exposed as one Concord dialect. It is not five SDK copies of a bug. The current Salesforce profile explicitly uses `escape(None)` (`examples/salesforce-sdk/src/csv.rs:162`), so this report does **not** claim its normal workflow is corrupt. The common library's supported opt-in profile is affected; static profiles resolve into the same configuration.

**Smallest credible fix:** make the writer escape literal escape bytes as required by the reader, including around quotes. A temporary fail-closed fix can reject fields containing the configured escape instead of emitting corrupt data; rejecting all escape dialects would unnecessarily remove valid reads. Test literal escapes, repeated escapes, a trailing escape, and escapes before quotes/delimiters for both quote modes. Merely rejecting `escape + double_quote(true)` does not fix the reproduced `false` case.

**Different representation:** serialize once into bounded raw fields plus a schema, then encode those fields with a single explicit escaping policy; alternatively separate read-only permissiveness from the writer's canonical escape dialect. This makes the supported inverse relationship explicit, instead of assuming two builder options mean identical transformations.

**Impact:** fix changes wire bytes for affected values, or turns formerly successful lossy encodes into errors; keep public signatures and default dialect unchanged. Extra scanning/escaped output is linear in field size; no new dependency is inherently required. Header serialization needs the same rule. Affects custom CSV integrations and any future provider using alternate escapes; no demonstrated effect on GitHub, Stripe, Shopify, Kubernetes, or Salesforce's current default profile.

**Classification / severity / evidence:** correctness defect / **high** / **reproduced**. High because valid input silently changes, scoped to opt-in escape profiles. **Disposition:** fix now. **Falsifier:** the same successful encode/read round-trip preserves the exact original bytes in both modes, or the unsupported value is explicitly rejected before being presented as a successful encoding.

## ASTRA-2 — CSV record/header limits do not bound writer allocation

**Affected task:** upload or spool a large row under a configured small record limit, expecting the encoder to stop resource use near that limit.

**Locations:** `crates/concord-extras/src/csv/write.rs:263` appends to an uncapped sink; `write.rs:247` copies its complete buffer to output; `write.rs:352` serializes/flushes a whole header before checking at `write.rs:362`; `write.rs:406` serializes a whole row, copies at `write.rs:419`, then checks at `write.rs:422`. Lazy transport calls the same encoder at `crates/concord-extras/src/stream/upload.rs:224`. Consumer: `review/astra/task/src/bin/writer_memory.rs:23`; provider exposure: `examples/salesforce-sdk/src/csv.rs:247` and `csv.rs:326` (batch spool).

**Reproduction:** two isolated processes, inside `review/astra/task`:

```sh
CARGO_TARGET_DIR=../target cargo run --offline --bin writer_memory
CARGO_TARGET_DIR=../target cargo run --offline --bin writer_memory -- stream
```

Both use a 16 MiB string field and `max_record_bytes(64)`. The RSS baseline is sampled **after creating the input**. Finite encoding reports the correct refusal, but process high-water RSS grows **32,540 KiB**; direct polling of a lazy upload reports a body error after **32,648 KiB** growth. Logs: `logs/writer-memory-finite.log`, `logs/writer-memory-stream.log`. These are Linux process high-water observations, not exact allocation counts; source inspection independently shows two full-sized library buffers. No HTTP/transport buffering participates in this probe.

Expected: reject before library-owned output buffers grow with the rejected 16 MiB field, allowing fixed serializer scratch, the configured record cap/terminator, and any previous bounded frame. Actual: the record limit is an acceptance check after allocation, not a memory bound. Header code has the same ordering by source inspection; that branch was not separately RSS-measured. `max_fields` also checks only after the shape pass (`write.rs:397`, `write.rs:443`), so it is not a bound on work performed by arbitrary serializers.

**Cause/recurrence:** one shared writer, not independently duplicated encoder code. Salesforce spooling uses the finite writer and checks its body limit after a batch; it inherits this transient behavior. The small-record stress upload does not exercise oversized records. JSONL writing explicitly has no writer size limit (`jsonl/writer.rs:116`); do not mistake its reader limit for one. That is a documented separate limitation, not evidence that this CSV option is enforced before growth.

**Smallest credible fix:** give `Sink` a per-record/header budget, check before `extend_from_slice`, reset it at record boundaries, and distinguish budget exhaustion from generic serialization/I/O errors so numeric limit diagnostics survive. Bound allocation growth, not only length. Do not copy an already-refused record into the outgoing frame. A small fixed internal `csv::Writer` buffer can remain outside the variable budget and be documented.

**Different representation:** a bounded `Write`-based record encoder, usable with memory, files, or a spill-to-disk spool, would also address Salesforce's known-length upload need. It avoids collecting batches solely to acquire bytes and reuses one schema/header state throughout a spool. This is larger than the immediate sink fix.

**Impact:** no required public signature, macro, or dependency change; accepted bytes can remain identical. Failures occur earlier within the same encode/body poll. Runtime adds a cheap remaining-budget check and reduces allocation/copies; no substantial compile-time cost. A new writer/sink API would be additive and deserves separate API design. Affects all CSV write routes, particularly Salesforce and custom bulk upload pipelines; does not invalidate bounded incremental reads.

**Classification / severity / evidence:** performance-resource limit / **medium** / **reproduced**. The docs explicitly promise pre-growth bounds for readers, not writers, so this is not labelled a contradicted reader contract. **Disposition:** fix now. **Falsifier:** with preallocated inputs of increasing size and a fixed 64-byte cap, encoder-owned transient allocation remains bounded by that cap plus documented fixed scratch/frame overhead, and errors still identify the configured limit.

## ASTRA-3 — Empty chunks can monopolize the executor

**Affected task:** consume a CSV/JSONL/JSON-values source that returns ready empty chunks, while other jobs, deadlines, or cancellation share the executor.

**Locations:** `crates/concord-extras/src/stream/source.rs:14` explicitly permits empty chunks; `crates/concord-extras/src/stream/driver.rs:82` loops, and `driver.rs:84` immediately continues for every empty chunk. All three reader `next()` methods await this refill. Consumer: `review/astra/task/tests/probes.rs:71`.

**Reproduction:** inside `review/astra/task`:

```sh
CARGO_TARGET_DIR=../target cargo test --offline --test probes one_poll_consumes_a_million_empty_chunks_in_every_reader -- --nocapture
```

The `ByteSource` returns 1,000,000 ready empty chunks followed by `1\n`. A manual **single** poll of each reader's future drives **1,000,001** source calls and returns the record without returning `Pending` once. Expected: bounded source polling before waking/yielding, even though no input bytes were consumed. Actual byte-work counters remain irrelevant inside `refill`. An endless ready-empty source would never return from that poll (inferred from the loop; the reproduction deliberately terminates instead of hanging the test runner).

**Cause/recurrence:** common foundation used by CSV, JSON Lines, and JSON values; three reproductions are not three independent implementations. No current SDK's normal reqwest stream was shown to produce an endless empty sequence. The defect is directly reachable through the public `ByteSource` contract, including adapters for files/transforms. There is no basis here to claim a live provider exploit or remote empty-HTTP-frame mapping.

**Smallest credible fix:** count source completions that make no byte progress and call the existing cooperative `yield_now` after a bounded number, preserving the empty-chunk meaning. EOF must remain `None`; treating an empty chunk as EOF would corrupt input. No runtime-specific timer is necessary.

**Different representation:** a poll-based byte-source interface can expose polling budgets more directly, or the driver can return an explicit “no data yet, retry later” step to the reader. Either is more invasive than an internal no-progress budget; removing empty chunks from the public contract would break existing adapters.

**Impact:** internal, source-compatible change; same records and errors, additional wakes only on long empty runs, no new dependency, negligible state/compile cost. Helps all reader consumers, including Shopify exports, Salesforce parts, and Kubernetes watches when they use custom sources. Cancellation becomes observable between budgeted batches; it cannot preempt arbitrary user code inside one source poll.

**Classification / severity / evidence:** performance-resource limit / **medium** / **reproduced**. **Disposition:** fix now. **Falsifier:** the probe returns `Pending` after a bounded number of empty completions and later still delivers `1`; a ready-empty source cannot indefinitely prevent another cooperative task from being polled.

## ASTRA-4 — JSON framing applies the work budget too late

**Affected task:** parse a large unfinished JSON record/value from a large ready byte chunk without delaying unrelated jobs or cancellation beyond the advertised 64 KiB framing budget.

**Locations:** `crates/concord-extras/src/jsonl/lines.rs:505` passes the entire pending chunk to `Framer`; `lines.rs:151` searches its whole remainder for LF before checking the line limit; `lines.rs:570` yields only afterward. `crates/concord-extras/src/jsonl/values.rs:124` bounds the scan by remaining value size, not the work budget; `values.rs:444` passes the full pending chunk and yields at `values.rs:452` afterward. Public promise: `jsonl/mod.rs:52`, `values.rs:428`. Control: `csv/read.rs:212` slices work before parsing. Consumers: `review/astra/task/tests/probes.rs:107` and `probes.rs:125`.

**Reproduction:** inside `review/astra/task`:

```sh
CARGO_TARGET_DIR=../target cargo test --offline --test probes json_readers_exceed_the_documented_64k_poll_budget_on_one_partial_record -- --nocapture
CARGO_TARGET_DIR=../target cargo test --offline --test probes jsonl_large_unfinished_chunk_work_probe -- --nocapture
```

For an 8 MiB unfinished string in one chunk, the first poll consumes **8,388,608 bytes** in each JSON reader before yielding; CSV consumes **65,536**. No completed record or user deserialization participates. Separately, JSON Lines with a **64-byte** record cap searches a **32 MiB** unterminated chunk before returning `Limit`; one debug run took 99.7 ms. The timing is illustrative debug-build evidence, not a release-performance benchmark. The full-chunk search is the decisive source fact. Logs: `logs/probes-final.log`.

Expected: enforce the advertised work budget before scanning/copying a large partial record, and detect an oversized line after bounded lookahead rather than scanning arbitrary remaining input for a terminator. Actual: allocation is still bounded for accepted partial records, but time spent in a poll can scale with the supplied chunk; JSONL's rejected-chunk scan can exceed even the configured line limit.

**Cause/recurrence:** both JSON reader implementations apply the byte-work counter after a framing call. They are separate scanners sharing a pattern, not copied SDK adapters. CSV already demonstrates a suitable bounded-step implementation. Provider response chunk sizes are outside these APIs' guarantees; the reproduction uses the supported `Chunks` source, not a claim that current loopback reqwest emits 32 MiB chunks.

**Smallest credible fix:** limit each framing input slice to the remaining work budget, retaining the rest in the driver; cap LF lookahead by the remaining record capacity plus terminator/BOM allowance. Preserve true EOF versus a slice boundary. Keep cancellation state in the reader before yielding. Do not claim to preempt a caller's `Deserialize` implementation; that is a different cost.

**Different representation:** explicit resumable scanner steps returning `Progress`, `Record`, `Limit`, and `NeedInput` with consumed-byte counts, driven by one shared work budget. This resembles the CSV table/driver separation and avoids format-specific interpretations of “one poll.”

**Impact:** no mandatory public API/dependency changes. More wakes and possibly more buffered copies for very large values, so benchmark throughput and retain the small-record zero-copy path where possible. No changed JSON semantics or error redaction. Relevant to Shopify bulk JSONL and Kubernetes watches; ordinary small records and observed first-record latency remain good.

**Classification / severity / evidence:** performance-resource limit / **medium** / **reproduced**. **Disposition:** fix now. **Falsifier:** both partial-record probes consume no more than the chosen budget per poll, and an oversized-line refusal's framing work no longer grows with arbitrarily larger chunks under a fixed small limit.

## ASTRA-5 — Metadata preservation repeats native plumbing and impossible branches

**Affected task:** keep request/rate/version/status headers beside a typed provider value, including on later decode/body-read failure, while preserving Concord's endpoint context and safe transport diagnostics; extend an SDK with an undeclared native operation.

**Locations:** `crates/concord-core/src/prepared.rs:58` returns only `T`; `prepared.rs:86` offers the necessary native parts; `prepared.rs:291` is the only public decoder transport-conversion path and returns `Result<T, Error>` even for a known `Err`. The direct constructor is private at `crates/concord-core/src/error.rs:85`. Caller: `review/astra/task/src/lib.rs:5`. Integration implementations: GitHub `examples/github-sdk/src/meta.rs:219`; Stripe `examples/stripe-sdk/src/send.rs:452`; Salesforce `examples/salesforce-sdk/src/transport.rs:39`; Shopify `examples/shopify-sdk/src/context.rs:147`. Independent native extension: `checks/maintenance-github/src/issue_events.rs:180`.

**Reproduction:** `CARGO_TARGET_DIR=../target cargo test --offline --test caller provider_typed_value_keeps_metadata -- --nocapture` inside the scratch crate. It works, but the public path needs execute/match/capture/decode scaffolding and `.finish(Err(error)).await.err().unwrap()` to obtain an error whose existence the type does not express. GitHub uses `unreachable!`; Stripe constructs an impossible success with status 0; Salesforce handles an impossible success with no usage; Shopify can forward the `Result` because its output shape matches. The independent GitHub maintenance extension repeats the same branch outside the SDK, and loses the generated endpoint label when constructing a fresh `Decoder`.

Expected ergonomic capability: execute/capture/decode once with typed failure stages and no invented success case for a known transport failure. Actual HTTP behavior is correct; this is **not** a runtime data-loss claim. Simply saying the native recipe works does not eliminate the repeated caller/maintainer work.

**Cause/recurrence:** the SDKs deliberately adapt the documented native recipe (GitHub and Stripe say so). These source occurrences alone are not four independent discovery experiments. They nevertheless serve different provider needs and have different concrete branches. This review's blind caller task independently encountered the same missing conversion seam; the non-author GitHub maintenance exercise records another native-extension occurrence (`MAINTENANCE.md:227`, `:243`, `:348`). That evidence supports a shared primitive; it does not support putting all provider metadata/error documents into core.

**Smallest credible fix:** expose synchronous `Decoder::transport_error(reqwest::Error) -> Error`, using its existing endpoint metadata and URL removal. This removes the impossible branches immediately. For the full recurring task, propose a decoder-level head-capture helper (and optional `Prepared` shorthand), returning selected `M` with `T`, or the error with `Option<M>` once a head exists. A helper only on `Prepared` would miss the independently reproduced native-extension task. Keep non-2xx/metadata response ownership and caller-selected redaction intact.

**Different representation:** an explicit opened-response value owning the native response and decoder, with a borrowed-head inspection stage followed by typed finish. A custom `Decode<WithMeta<T, M>>` wrapper already works for author-selected metadata (`crates/concord/tests/metadata.rs:118`), but changes the declaration's output/format and does not give callers arbitrary metadata capture on an existing `Prepared<T>` or retain metadata on failed decoding.

**Impact:** additive core APIs; no macros, serializer dependencies, or provider-specific header schemas required. Conversion is the existing operation exposed synchronously. A capture callback monomorphizes per caller and stores only selected owned metadata; cloning every header/URL by default would impose avoidable cost and redaction risk. Public error/wrapper `Debug` must not automatically print `T` or `M`. GitHub, Stripe, Salesforce, and Shopify benefit; Kubernetes can continue `send_raw` + `open_for`. Provider-specific non-2xx documents, signed URL validation, retries, 304 policy, and streaming recovery should stay outside the helper.

**Classification / severity / evidence:** developer friction / **medium** / **reproduced**, with source-inspected recurrence. **Disposition:** propose for next release; no core rewrite needed to complete current workflows. **Falsifier:** an existing documented public API can preserve selected metadata and the endpoint decoder for both prepared and independently built native requests, convert a known transport error directly, and retain captured metadata after decode failure without the scaffolding above—or an adoption experiment shows the proposed helper fails to remove that repeated code.

## Counter-evidence and boundary assessment

**Core and ownership.** Status precedes format checks; rejected heads retain unread responses (`prepared.rs:242`, `stream/error.rs:127`). My held-body task and existing `crates/concord/tests/metadata.rs:91`, `:189`, `:217` passed. `Decoder` erases only the format/head step, retains the declared output, and permits request edits without silently changing its decoder. No concurrency/ownership defect was found in this seam. The finite typed path explicitly collects at `prepared.rs:264`; a no-output endpoint instead drains chunks at `:249`. Consequently `send()` on a watch is wrong for receiving events, even though it does not collect them. Kubernetes correctly uses `send_raw` and `JsonValuesReader` (`examples/kubernetes-sdk/src/watch.rs:264`).

**Actual streaming and first record.** CSV and JSONL deliver before the body ends, pause with the consumer, preserve earlier records on late failure, and release connections when dropped. Re-run tests include `csv_http.rs:323`, `:371`, `:439`, `:551`; `jsonl_http.rs:30`, `:153`, `:196`; and `jsonl_incremental.rs:25`, `:62`, `:98`. There is no whole-response collection inside these reader paths. Salesforce's `ResultSets` intentionally returns a collected page; its lower-level response/reader route and the independent fanout sink exercise consume rows incrementally (`checks/dx03-fanout-sink/src/stage2.rs`, discussed at `NOTES.md` “What was easy”). This distinction matters more than whether a public type name contains “stream.”

**Fused state and retention.** `Driver::fail` clears pending input and fuses (`stream/driver.rs:138`); reader `next()` checks this before work. Parser buffers and the source remain owned until dropping the reader or taking `into_source`, so a terminal error is not automatic destruction. That permits deliberate partially-consumed-source access. Retained CSV capacities are reused; JSON accessors count lengths, not capacity (`csv/read.rs:246`, `jsonl/lines.rs:584`, `jsonl/values.rs:500`). The docs identify these differences and exclude transport chunks, kept headers, delivered values, and application queues. I found no unbounded record-history queue or background reader task. A reused large buffer is retained memory, not by itself a leak.

**Limits.** CSV framing checks probes before extending beyond record/header/field capacities (`csv/frame.rs:279`, `:315`); limits exactly at boundaries and across splits pass (`tests/csv_limits.rs:117`). JSONL/value framing bounds retained unfinished records; total-byte checking is intentionally per incoming chunk (`stream/driver.rs:86`, `jsonl/policy.rs:296`). A chunk crossing the total cap can therefore be refused before yielding an otherwise complete prefix in that chunk; the documentation describes that policy. These limits do not bound user deserialization, returned data, transport allocation, or total finite-body collection. ASTRA-2 and ASTRA-4 distinguish allocation limits from acceptance and work limits.

**One-shot uploads.** `RecordBody` pins its source in a box and stores state behind `Mutex`, allowing a `Send` source without `Sync` or `Unpin` (`stream/upload.rs:148`, `:162`, `:174`). My non-Sync/non-Unpin source compiled and cancelled correctly. There is no source clone/retry or whole-source collection in `StreamRows`/`StreamRecords`. The body frame target is 16 KiB, not a hard record-size bound. `from_iter` separately requires the iterator to be `Unpin` (`stream/upload.rs:288`); that convenience-adapter restriction is real but did not obstruct the direct-stream task and is not evidence that upload sources universally need `Unpin`. Finite `Rows<I>` encodes eagerly, as documented.

**Partial failures and error typing.** Upload failure is a Concord `Transport` error at the send boundary, with an inspectable `UploadError` deeper in the explicit transport source chain; it is not a rollback or proof of how many records the peer committed. The body counts serialized records, not peer acknowledgments (`stream/upload.rs:96`, `:232`). Existing paced failure tests passed (`csv_http.rs:851`, `:948`). Reader `ReadError` distinguishes transport, grammar, typed decode, and limits; `RecordErrors::Yield` continues only for isolated delimited JSONL errors, and terminal failures report `is_terminal()` (`stream/error.rs:168`, `jsonl/lines.rs:552`; tests `jsonl_framing.rs:796`). Callers still own durable sink staging/publication; the independent fanout task tests that boundary successfully.

**Redaction.** Core errors remove the top-level reqwest URL and withhold untrusted source chains (`crates/concord-core/src/error.rs:85`, `error.rs:200`, `error.rs:292`); `CodecError` exposes static labels/numeric positions with deliberate unredacted-source access (`crates/concord-core/src/codec.rs:284`, `codec.rs:350`). Stream errors preserve that design (`stream/error.rs:257`, `stream/error.rs:303`). Runtime mismatch and malformed-body tests pass without data appearing in ordinary diagnostics. This has a diagnostic cost—the Salesforce maintenance task had to inspect the source to see a missing field name—but printing arbitrary Serde errors by default would leak field values. I do not recommend weakening the default. Native requests, response headers, and explicit source access remain unredacted by design.

**CSV type/runtime and media semantics.** The type identifies header and delimiter policy; it is not a proof of dialect validity (`Csv<_, Delimiter<b'\n'>>` is a valid Rust type rejected by configuration validation). Profiles cannot override the type's H/D, but other option interactions are validated at preparation/open. Runtime options are necessary for Salesforce job dialects and worked in both independent CSV tasks. Named reading permits column order changes; named writing validates order and does not reorder fields (`csv/write.rs:451`; maintenance-salesforce `MAINTENANCE.md:219`). Explicit positional/raw rows provide a different representation when job columns dictate order. Header means first parsed record, including quoted newlines; empty headed input is an error, header-only is zero rows, headerless first input is data (`tests/csv_mapping.rs:96`, `:225`, `:350`). These rules are deliberate and tested, not reasons to infer configuration from content.

CSV checks `header=` as content-type metadata; `IgnoreContentType`/unchecked media deliberately skips it, not table validation (`csv/codec.rs:50`, `tests/csv_http.rs:176`). JSONL's media family and optional aliases differ appropriately from consecutive JSON values' JSON media, including `+json`; no automatic cross-format selection occurs. Static upload media is compatible with `Encode`; runtime `Upload::apply` pairs body and header (`stream/upload.rs:47`). Salesforce's concern about whether a live service accepts `header=present` is explicitly unverified in its evidence; it is not promoted here to a reproduced core compatibility defect. Likewise strict CRLF-only validation would be new functionality, not a demonstrated violation of the currently documented permissive reader.

**Scope of core changes.** GitHub continuation URLs, artifact redirects, Shopify signed downloads and GraphQL response envelopes, Salesforce runtime CSV and known-length spooling, and Kubernetes watches all fit public native paths. Bounded provider-error-body loops recur in Stripe/Salesforce/Kubernetes (`stripe-sdk/src/send.rs:498`, `salesforce-sdk/src/error.rs:187`, `kubernetes-sdk/src/error.rs:100`), but their document/error retention policies differ; a reusable bounded-byte utility is a smaller candidate than putting provider documents in core. The strongest independently repeated core request in this review is ASTRA-5. SDK context omissions and duplicated resource-option forwarding from maintenance exercises should not be “fixed” by broadening codec traits.

## Verification and stress evidence

All repository commands used `--offline --locked`, `CARGO_TARGET_DIR=$PWD/review/astra/target`, and `TMPDIR=$PWD/review/astra/tmp`. No source tree outside the assigned directory was edited.

| Command scope | Passed | Failed | Ignored | Log |
|---|---:|---:|---:|---|
| `cargo test -p concord-core -p concord-extras -p concord-example-native --all-features` | 217 | 0 | 1 | `logs/core-extras-native.log` |
| Five provider SDK packages + `concord-check-dx03-csv-runtime` + `concord-check-dx03-fanout-sink` | 154 | 0 | 0 | `logs/providers-callers.log` |
| Five `concord-check-maintenance-*` packages + `concord-check-stream-stress` | 47 | 0 | 0 | `logs/stress-maintenance.log` |
| `cargo test -p concord --all-features --test metadata --test error_inspection --test bodies` | 26 | 0 | 0 | `logs/facade-boundaries.log` |

Total: **444 existing test/doc-test cases passed**, one opt-in throughput measurement ignored. Independent scratch suite: **3 caller tests + 4 review probes**; two isolated memory processes also completed with the observed resource issue. No claim of a complete workspace/all-app test run. The later probe-writing syntax error is preserved verbatim in `logs/probes-budget-compile-error.log`; it was an extra `>` in review code, fixed by a generic helper, unrelated to Concord. The initial caller-task fixture failure remains in `logs/caller-first.log`.

I inspected the supplied `/home/canarit/projects/concord-ext44/reviews/stress-256-54f96ab.json`, not a newly run 256 MiB benchmark. `git diff 54f96ab 0dd65501 -- crates/concord-core crates/concord-extras checks/stream-stress` is empty. The measurements therefore apply to unchanged code in this scope, subject to their recorded environment and workload:

- JSONL 256/512 MiB reads: first record about **0.554/0.233 ms**, **706/761 MiB/s**, peak unfinished-record length **127 bytes**. CSV: about **0.389/0.361 ms**, **548/610 MiB/s**, peak framing allocation **384 bytes**. Both first records precede body completion by hundreds of milliseconds.
- All measured uploads have **0 source polls before send**. Cancellation observations show no source-poll increase 200 ms afterward; read cancellation shows stable generated bytes. The current-commit stress smoke tests passed, and the independent task reproduced CSV upload cancellation against a slow receiver.
- These workloads generate **128-byte read records**, **64 KiB chunks**, and a **4096-byte reader cap**. They establish streaming and bounded behavior for that workload, not pathological-record or arbitrary-chunk bounds. RSS is whole-process sampling every **20 ms** (`checks/stream-stress/src/lib.rs:204`), with reused process allocation between cases. Upload `peak_retained_bytes=0` is a default/uninstrumented field (`lib.rs:537`), not proof of zero encoder memory. ASTRA-2 measures a transient case this harness cannot establish; ASTRA-3/4 probe input/work cases it does not generate.

Fixture manifests for all five apps were inspected for provenance/evidence scope. They identify synthetic/schema-derived or documentation-example bodies, injected faults, and pinned provider references; they are not live service captures proving current provider acceptance. Maintenance evidence is particularly useful for distinguishing real boundary repetition from frozen-consumer copying: GitHub's native extension repeats transport/metadata handling; Shopify's ~110 copied consumer lines mostly arise from the exercise's frozen helpers; Salesforce's dialect change needs no parser edit; Kubernetes reuses list/watch machinery unchanged. The re-run maintenance and independent caller suites support those narrower conclusions.

The proposed fixes are review recommendations only. No library or SDK implementation was changed.
