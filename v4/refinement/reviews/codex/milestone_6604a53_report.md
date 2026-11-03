# Independent milestone review — frozen source 6604a53

**Recommendation: revise the four confirmed issues below.** No high-severity core wire-composition, ownership, or sensitivity failure was confirmed in the exercised cases. This is a scoped review recommendation for coordinator triage, not milestone acceptance.

**Identity and execution environment.** Original commit: `6604a53be4e4c92d4e97ae2b71925902565e31fd`; original tree: `a84a014771a70dce4104472f9660f28163fe2731`. Independent snapshot HEAD: `a3b5407f92ac7d9b7f6cfd5cdf59de0ad0baec1f`; snapshot tree: `69161ff73a27946806ac3b2c7e3278b28dde33fa`. All 229 origin-manifest files match their Git blob IDs and executable modes. Reconstructing the manifest's Git tree produces the recorded original tree. The snapshot's distinct HEAD/tree are expected. The supplied change range is `8f7adc2..6604a53`; the old commit history/source is not available here, so findings are attributed to the frozen source, not asserted to have first appeared in a particular commit.

Mode: execute, Linux x86_64, workspace-write sandbox, approval policy `never`, network enabled for loopback. Effective session `turn_context` verifies model `gpt-6.1-sol`, effort `xhigh`, including the same collaboration-mode settings. This is observed session metadata, not merely the requested launch arguments or global defaults. See [effective settings](effective-settings.json). Rust: `1.97.1 (8bab26f4f 2026-07-14)`; Cargo: `1.97.1 (c980f4866 2026-06-30)`; MSRV runs used `cargo +1.85` (installed 1.85.1 toolchain). `CARGO_HOME` was this snapshot's `target/cargo-home`; `CARGO_TARGET_DIR` was `target/review-build`; build jobs were 6. Root lockfile SHA-256: `f6782007719be5934dcf9e1d39012e7e659cd4e08cfdcba357d19852cb72a919`.

No external service was contacted, no push/publication/global-setting change was performed, and no sandbox bypass was requested. OpenAI Docs skill instructions were read for the settings question; external documentation lookup was omitted under the explicit no-external-services restriction. No agents were delegated. No production source was changed. All new files and mutation copies are under `target/probes/`; the local documentation server was stopped. See [source integrity](source-integrity.json).

**Personally executed evidence.** The fresh baseline target directory did not exist before the build. The first build of the unmodified source was `cargo build --workspace --all-targets --locked --offline`, exit 0. The log identifies the snapshot's crate paths; subsequently executed tests were built by Cargo from those paths. No cached executable was substituted. [Baseline build](baseline-build.log) and [binary paths/hashes](baseline-binaries.json) retain provenance. Later scratch-copy builds are explicitly identified below and in their logs.

| Command / evidence | Result and exact scope |
|---|---|
| `cargo test --workspace --locked --offline` | 87 Concord runtime tests pass, then UI harness fails on one path-normalization mismatch out of 51 cases. This command did **not** pass as a whole. [Log](workspace-test.log) |
| `cargo test -p concord --locked --offline --test values` | Remaining 15 runtime tests pass. Together with the preceding run, all 102 current Concord runtime tests executed successfully. [Log](values-test.log) |
| `cargo test --workspace --locked --offline --exclude concord` | 36 tests pass: native examples 29, shapes 3, renamed consumer 1, heap check 1, macro/base-consistency tests 2. [Log](downstream-existing.log) |
| `cargo +1.85 test --locked --offline -p concord --lib --test attributes --test values --test composition --test display_errors --test native_limits --test representations --test bodies --test sensitivity --test hygiene --test slice --test tls` | 102 runtime tests pass on MSRV. UI snapshots were not asserted compatible with this compiler. [Log](msrv-runtime.log) |
| `cargo test -p concord --locked --offline --doc` | Five doctests compile successfully; these are compile/no-run examples, not five network tasks. [Log](doctest.log) |
| `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | Both pass on frozen production sources. [fmt](fmt.log), [clippy](clippy.log) |
| `cargo doc --locked --offline -p concord -p concord-example-shapes -p concord-example-native --no-deps` | Builds generated HTML. Default Concord docs also pass with `RUSTDOCFLAGS='-D warnings'`. [HTML build](rustdoc.log), [strict default](rustdoc-default-strict.log) |
| `RUSTDOCFLAGS='-D warnings' cargo doc --locked --offline -p concord --no-default-features --no-deps` | Fails with two unresolved `Json` links; F-03. [Log](rustdoc-no-default.log) |
| `cargo test --manifest-path target/probes/downstream/Cargo.toml --lib --locked --offline` | Eight independent downstream tests pass with renamed dependency `co` and no default features. The occurrence-table oracle checks 729 input states through two branches: 1,458 preparations, exact query order, per-name header values, sensitivity flags, and diagnostic positive/negative controls. [Source](../downstream/src/lib.rs), [log](independent-probes-final.log) |
| `cargo test --manifest-path target/probes/json-only/Cargo.toml --offline` | Isolated JSON-only consumer passes an independent byte/decode test. Consumer lockfile seeded from frozen lockfile; first resolution adds only the consumer package. Subsequent `cargo tree ... --locked --offline -e features -i reqwest` shows no reqwest default feature enabled. The no-default consumer has the same isolation property. [JSON test](json-only.log), [JSON features](json-only-features.log), [no-default features](no-default-features.log) |
| `cargo run --manifest-path target/probes/native/Cargo.toml --offline` | Actual loopback upload through the public native example; response decodes successfully. Independent Python MIME parsing reveals parameter corruption, F-01. [Rust source](../native/src/main.rs), [wire bytes](multipart-wire.mime), [parsed result](multipart-parsed.json) |
| `cargo check --manifest-path target/probes/downstream/Cargo.toml --bin raw_client --offline`; corresponding `--bin migration --locked --offline` | Expected counterexamples fail, respectively raw root name generation and the maintained migration constructor. [Raw client](raw-client.log), [migration](migration-snippet.log) |
| `cargo +1.85 check --manifest-path target/probes/downstream/Cargo.toml --bin header_limit --locked --offline` | The 65,536-byte header-name declaration is rejected with E0080 on MSRV; current pinned-compiler UI test rejects it too. The 65,535-byte positive boundary passed in both runtime runs. [MSRV failure](header-limit-msrv.log) |
| `cargo package --locked --offline -p concord --list` and `-p concord-macros --list` | Both lists inspected: production dependencies/features and local package contents. No archive installation, registry resolution, publication, or release validation claimed. [Runtime list](package-concord.log), [macro list](package-macros.log) |

The initial UI mismatch is environmental: trybuild first normalizes the workspace prefix, so the nested registry path becomes `$WORKSPACE/target/cargo-home/.../http-1.5.0/...` instead of the snapshot's `$CARGO/http-$VERSION/...`. The diagnostic remains E0080 at the same invocation. A source-identical copy rooted at `target/probes/mutant-src` makes that registry external to its workspace, and all 51 UI cases pass with `cargo test --manifest-path target/probes/mutant-src/Cargo.toml -p concord --test ui --locked --offline`. The final [restored-copy run](ui-relocated-restored.log) was performed after mutations had finished; an earlier concurrent scratch-copy UI run is not needed for this conclusion. This is not a claimed production compiler defect or a silently blessed stderr file.

**Mutation discrimination.** Only copies of source under `target/probes/mutant-src` were mutated. Its workspace member list was narrowed to the two Concord crates and the heap-check package; its settled lockfile contains a subset of the original package/version/source entries, with no new dependency versions. The independent consumer points explicitly to this copy. Each final mutation compiled, reached a test assertion, failed, and was restored; restored eight-test and heap suites pass. [Harness](../mutations.py), [receipt](mutation-receipt.json), [run](mutation-run.log).

| Deliberate mutation | Discriminating result |
|---|---|
| Ignore optional overrides and always emit the inherited fallback | Oracle fails at minimized input seed 11: actual `k=v0a&k=v0b`, expected no pairs. This exposes present-empty replacement, rather than merely checking a nonempty happy path. [Failure](mutation-optional-override.log) |
| Skip effective-sensitivity propagation | Branching graph test fails because generated client Debug exposes a value connected to a sensitive key on another branch. [Failure](mutation-sensitivity-propagation.log) |
| Buffer unit responses instead of draining chunks | Heap test fails: unit peak growth 136,819,854 bytes in the corrected first run, rather than below 4 MiB. Restored run: unit 754,709 bytes; Binary 137,344,090 bytes for a 64 MiB body. The Binary control detects buffering. [Failure](mutation-buffer-unit.log), [restored](mutation-restored-heap.log), [first corrected receipt](mutation-heap-receipt.json) |

Two probe-setup failures are retained, not counted as defects: the first encoding probe incorrectly tried to put Unicode in an ASCII-only header; it was corrected to assert rejection and test the accepted ASCII case. The first drain-mutation attempt was blocked by the narrowed copy's unsettled lockfile (`cannot update the lock file ... because --locked was passed`), so it supplied no drain evidence. The copied lockfile was settled offline without changing dependency versions, a clean heap baseline passed, and the final full mutation sequence above passed all expected-exit checks. [Initial invalid receipt](mutation-receipt-initial-invalid.json), [execution exits](execution-sessions.json). No tracked snapshot file or root lockfile was altered.

**Confirmed findings.** P2 means moderate functional/documentation impact; P3 means a narrower defect. None is asserted to be newly introduced in the supplied commit range without the unavailable old source.

**F-01 — P2 — native multipart recipe corrupts backslashes in field names and filenames.** Location: `examples/native/src/bodies.rs:70–79`, `header_safe` and its use inside quoted `Content-Disposition` parameters. Minimal input: `FilePart { field: r"file\name", file_name: r"report\q.csv", content_type: "text/plain", content: b"hello" }`, sent with `upload_file`. Expected: a MIME recipient receives field `file\name` and filename `report\q.csv`. Actual independent MIME parsing receives `filename` and `reportq.csv`: the unescaped backslash is a quoted-pair escape. The application may submit under the wrong form field or silently rename the file. Reproduction: the native probe command above exits 0 after a real loopback upload; `multipart-parsed.json` records the mismatching names. Static support: the locked reqwest multipart implementation explicitly escapes backslashes in filenames at `reqwest-0.13.5/src/async_impl/multipart.rs:557–561`, while this recipe does not.

After the native upload command, `python3 target/probes/check_multipart.py` exits 1 on an independent decoded-parameter assertion; [parser fixture](../check_multipart.py), [failure](multipart-assertion.log). Smallest correction: escape `\` as `\\` before writing quoted disposition parameters, keeping the chosen quote/CR/LF policy explicit. Alternatively, enable and use reqwest's native multipart feature in this application example and remove the manual builder; no new Concord responsibility is needed. Regression: send names containing backslashes (including one just before a closing quote) and assert their decoded parameters using an independent MIME parser. The existing byte-string expectation repeats the recipe's construction and does not detect this semantic error. This finding is confined to the example, not Concord's encoder or core HTTP execution.

**F-02 — P3 — raw-keyword client identifiers lose their raw spelling during generation.** Locations: `crates/concord-macros/src/model.rs:1356` and `:1436`. Minimal declaration: `co::api! { pub client r#type { base "http://probe.invalid"; GET ping(); } }`. Expected: ordinary Rust raw-identifier naming works and `r#type::new(...).ping().prepare()` compiles. Actual: `expected identifier, found keyword type`, followed by an invalid suggestion to spell it `r#r#type`. The parser accepted the declaration; `plain()` removes `r#`, then `Ident::new` reconstructs a keyword token. Reproduction: `cargo check --manifest-path target/probes/downstream/Cargo.toml --bin raw_client --offline`, exit 101; [fixture](../downstream/src/bin/raw_client.rs). This blocks a narrow legal naming choice and worsens the diagnostic; raw value/control names work in the positive probe.

Smallest correction: preserve the original `Ident` for the root generated type; use the plain spelling only when forming concatenated names and display text. Regression: a real renamed-dependency consumer with a raw-keyword client, with a module-level naming-lint allowance, plus the existing raw parameter and macro-forwarding cases. No broader naming convention change is required.

**F-03 — P3 — supported no-default-feature rustdoc has broken links.** Locations: `crates/concord/src/lib.rs:141` and `:184`. Minimal task: generate strict docs with no default features. Expected: docs describe the optional JSON feature without linking to a missing item. Actual: two unresolved links to `Json`; documentation command exits 101 under `-D warnings` (ordinary rustdoc warns). Default strict docs pass. Reproduction is the exact no-default doc command in the execution table. User impact: projects enforcing rustdoc warnings cannot document this supported configuration cleanly.

Smallest correction: use feature-independent code formatting for the optional name, or conditionally emit the linked prose when JSON exists. Regression: add the no-default documentation command to the existing documentation gate, alongside the default build. This requires no feature or runtime change.

**F-04 — P2 — maintained migration material still teaches the removed root constructor and body model.** Locations: `docs/MIGRATION.md:67`; `v4/SEMANTICS.md:63–64`; also the `Json<T>` feature comment in `crates/concord/Cargo.toml`. Minimal task: follow the current V4 example and call `ShopApi::new(http, token)`. Expected: maintained migration examples compile against this alpha's selected API. Actual: E0308, expected `ShopApiConfig`, found `String`. [Minimal compile fixture](../downstream/src/bin/migration.rs) and [result](migration-snippet.log) isolate the constructor from unrelated DTO setup. The normative values table still declares `body b: R` stored as `R::Value`, although the parser/runtime now require `body b: T via R` stored as `T`. That is static documentary contradiction, not permission to retain the obsolete model.

Smallest correction: use `ShopApiConfig { token }`, update the two body rows and feature description, and add a compact V4-alpha migration note for root config construction, generic representation traits, required optional-override spelling, and `into_response() -> Result`. Regression: compile the maintained after-migration snippet as a downstream fixture and audit the migration table against the actual public signatures. The coordinator explicitly notes M6 docs are pending; that explains this known incompleteness but does not make the current example correct. Maintained code callers in shapes, renamed, native and benchmark packages do compile; this finding concerns the remaining reader-facing path.

**Inspected-only evidence and design assessment.** I read the supplied charter, milestone note, neutral task corpus and regression matrix; the current refinement decisions and semantics; the macro parser/model/template/emitter; runtime construction, representation, prepared/decoder/error/value code; relevant tests and native recipes; README/migration text; feature manifests, CI and scale scripts. Historical review/benchmark assertions are not counted as personally executed evidence for this revision. There is no Windows execution receipt, no current performance comparison, and no human usability study here.

Root config structs address same-typed positional root inputs; scope/endpoint arguments remain positional with the documented swap risk. Explicit `via` gives payload types directly in rendered signatures. The no-Clone endpoint/body route and a 1 MiB clone-expensive value were exercised without hidden preparation clones; client/scope copy-on-write still requires Clone. The higher-ranked endpoint Clone trick works on both tested compilers; its recorded compiler-dependence is a design risk, not a reproduced current defect. Interior mutability remains caller-owned: an already-selected request observes a later atomic update at preparation, consistent with the structural-snapshot wording.

The native header-order correction is justified by HeaderMap's ownership of cross-name ordering; exact query order and same-name header value order remain tested. Requiring `else inherit` for optional overrides and making owner-local removals act first is a coherent, explicit selected rule; both source inspection and an independent imperative occurrence-table oracle support the tested outcomes. The oracle does not call the production composer, construct its Item tree, or use the final URL as its expected-result generator. It covers two short branches, not arbitrary graphs or scale.

Header-name const validation, fallible header-map insertion, failing Display handling, unread status response ownership, Decoder::finish, chunked unit draining, raw streaming/early drop, and native timeout/header edits have direct execution evidence. Header maximum tests prove constructor/preparation boundaries, not acceptance by every HTTP server. Native defaults remain native defaults; removing a Concord contribution does not remove a reqwest default. Custom `BodyError::description` is trusted caller code with a documented content-free requirement; the library cannot sanitize an arbitrary callback's chosen description. Error source-chain redaction tests include native positive controls and Concord negative controls.

Source inspection found no core auth/refresh, retry scheduling, rate admission or pagination traversal subsystem. The native recipe package implements application policies with explicit bounds and replay assumptions, using public APIs. That respects the charter. Removing the custom multipart serializer in favor of the native feature is an opportunity to reduce example maintenance, not a reason to add core syntax. No per-placement adapter, reset facility, scoped representation default, generic state container or managed executor is needed to correct these findings.

Generated HTML was built and its text/signatures inspected: root config, required setter, `maybe_cursor(None)` wording, initializer timing, `Prepared<Page>`/`Result<Page, Error>` and endpoint request summaries are present. [Extracted evidence](generated-doc-inspection.json). Attempting a rendered browser view returned **“The browser access host is unavailable.”** Visual inspection is BLOCKED; HTML text inspection is not presented as rendered QA.

**Contract-case disposition.** These IDs are from the supplied CONTRACT_CASES, not the older repository matrix. Every row refers to original source `6604a53be4e4c92d4e97ae2b71925902565e31fd` verified above. PASS is limited to the named assertion/selected route; it is not a universal correctness claim. Existing tests listed below live under `crates/concord/tests/` unless prefixed `native/` (meaning `examples/native/tests/`) or another path. Their executed logs are in the command table. `probe::` denotes the independently authored downstream source linked above. NOT_RUN rows retain their unexecuted part explicitly.

| ID | Status | Discriminating evidence / scope |
|---|---|---|
| V01 | PASS | UI `config_missing_field`, `missing_required_argument`; missing named field/argument diagnostics, not runtime defaulting. |
| V02 | PASS | `values::v01_required_root_value_empty_string_is_valid`: present empty root value retained; V01 separate missing case. |
| V03 | PASS | `values::v03_initializer_evaluation_points`, `v03_v09_initializer_order_and_clone_retention`: independent counters and declaration-order log, including immediate override. |
| V04 | PASS | Same counter tests plus `v10_request_clone_follows_the_data`: retained idempotency key/body, repeated prepare and actual send. |
| V05 | PASS | `values::v04_optional_absent_vs_present_empty`: absence versus empty occurrence. |
| V06 | PASS | `values::v05_required_optional_payload_is_not_omission`, `v06_nested_absence`; `composition::each_over_option_under_override` checks ordinary required Option iteration. |
| V07 | PASS | `values::v07_maybe_is_exact`: initialized optional explicitly absent and None inference. |
| V08 | PASS | `values::v10_snapshots_are_not_changed_by_later_parent_changes`, `required_values_have_setters`: old child/new parent wire values. |
| V09 | PASS | `probe::large_values_move_only_body_and_structural_snapshot`: selected child observes later shared atomic contents; no deep-freeze claim. |
| V10 | PASS | Endpoint move-only value/body, 1 MiB value with clone counter zero on prepare, body-bearing Clone; `values::v12_borrowed_string_and_literals` verifies accepted borrowed-string conversion. No general borrowed-lifetime API is promised. |
| V11 | PASS | Named root config in `hygiene::many_required_values`, missing-field UI; residual same-type positional scope/endpoint risk explicitly recorded in D01. |
| V12 | PASS | UI `conversions_are_closed`, `newtype_is_not_converted`; `values::v12_borrowed_string_and_literals` checks inference. |
| V13 | PASS | `values::c12_no_shared_state_between_clients`, `slice::m02_application_client_is_used`: separate Concord state and supplied native policy. |
| V14 | PASS | `probe::caller_local_typed_context_in_application_wrapper`: typed non-wire application context without fabricated placement. |
| C01 | PASS | `composition::c01_segments_compose_in_order` and base-prefix tests assert complete URLs. |
| C02 | PASS | `composition::c02_dynamic_segments_are_encoded_once`, mixed pchar case; independent encoded query-key/path probe. |
| C03 | PASS | `composition::c03_dot_and_empty_segments_are_rejected`, trailing-slash/base-unchanged tests. |
| C04 | PASS | `composition::c04_label_values_are_validated`, IP-base rejection, `dns_length_limits`; build errors, no panic. |
| C05 | PASS | Macro `base_consistency::corpus` and `random_structured_bases`; source proof that accepted static constructors use the same runtime parse; strict subset documented. |
| C06 | PASS | 65,535 positive runtime boundary on both compilers; 65,536 E0080 current UI and separate MSRV consumer. |
| C07 | PASS | `native_limits::header_map_capacity` two tests: repeated try_append exhaustion and auto content-type try_insert error. White-box bridge tests, not a 24,576-field DSL consumer. |
| C08 | PASS | `composition::d04_override_truth_table`, existing nested cases; independent oracle's 1,458 preparations compare a separate occurrence-table interpreter. |
| C09 | PASS | `composition::q10_present_empty_each_override_removes`; oracle mutant fails at seed 11. |
| C10 | PASS | `composition::q6_remove_reintroduce_append`, `remove_then_introduce_absent_removes`; oracle verifies retained first slot. |
| C11 | PASS | `composition::c08_suppressed_contributions_are_not_formatted`; Display counters on suppressed branches. |
| C12 | PASS | `composition::c09_header_slots_on_the_wire`, query-order tests; oracle checks per-name values, makes no cross-name header assertion. |
| C13 | PASS | `composition::client_default_headers_fill_only_missing_names`: actual transport sees native defaults when Concord omits. |
| C14 | PASS | `probe::foreign_id_with_three_explicit_wire_spellings`: NonZeroU64 wrappers produce path `ff`, query `id-255`, header `h255`. |
| C15 | PASS | Literal header UI, runtime header policy/content-type tests, Display error suite; errors do not print supplied text. |
| C16 | NOT_RUN | Long header input executed; current-revision deep/1,000-endpoint growth measurement not run. Generator/scripts inspected only. |
| R01 | PASS | `bodies::r01_post_json`, representations text/binary cases, no-output cases; isolated JSON bytes/decode counterexample. |
| R02 | PASS | `bodies::r02_no_body_vs_empty_vs_null` asserts recorded body/content-type behavior without inventing Content-Length: 0. |
| R03 | PASS | Downstream `native/repr` NDJSON and form implementations; native tests assert literal expected bytes and decoded rows. |
| R04 | PASS | Heap positive control plus buffering mutation failure and restored pass on copied source. |
| R05 | PASS | `representations::unit_drain_detects_truncated_body`: Transport, not false success. |
| R06 | PASS | `bodies::r05_status_error_keeps_the_response`, `representations::into_response_keeps_other_errors`; native problem-document test reads retained body/status. |
| R07 | PASS | `native/native_edit::extra_header_arrives_and_declared_decoding_applies`, timeout and status cases. |
| R08 | PASS | `native/streaming::early_stop_closes_the_connection_before_the_body_ends`, stall deadline and short-body cases on real chunked fixtures. |
| R09 | FAIL | Form and normal multipart cases pass; independent MIME parser exposes F-01 for supported parameter data. |
| R10 | PASS | `slice::m03_prepare_does_not_contact_the_server`; `native/policies::building_requests_performs_no_io` admission counter. User encoding/formatting callbacks remain caller code. |
| R11 | PASS | `native/policies::one_send_is_one_execute`; source has one native execute. No service-level exactly-once or redirect-free promise. |
| R12 | PASS | Native cursor/retry/admission/key-update tests execute public routes with no core policy additions. |
| S01 | PASS | `sensitivity::s01_s02_sensitive_values_are_sent`, `safety_list_normalization_is_exact`: wire-positive and Debug-negative controls. |
| S02 | PASS | Existing path/host/query/header redaction tests and `adv_2_effective_sensitivity_reaches_every_placement`; native prepared URL/header assertions, with header/query loopback controls. |
| S03 | PASS | Sticky override test and independent remove/reintroduce oracle assert sensitivity flags and withheld values. |
| S04 | PASS | `tls::c4_r_008_redirect_error_chain_does_not_leak`: native reflected-query positive control, Concord negative control. |
| S05 | PASS | `tls::c4_r_008_header_reflected_into_redirect_does_not_leak`: header-only reflected secret control. |
| S06 | PASS | `bodies::r03_malformed_json_is_a_decode_error`; native NDJSON error test and explicit raw-source boundary. |
| S07 | PASS | `probe::runtime_base_is_explicitly_outside_redaction`: base text visible, generated secret hidden, matching documented policy. |
| S08 | PASS | Error kind/status tests; native timeout test uses `transport().is_timeout()` without parsing raw text. |
| S09 | PASS | Faulty Display error tests and inspected `BodyError` content-free-description contract; arbitrary caller callbacks are trusted, not blanket-sanitized. |
| S10 | NOT_RUN | Independent branching fixpoint and sensitivity mutation executed; large chain/branch stress and a randomized sensitivity-graph oracle were not run. |
| T01 | PASS | Real renamed crate and macro_rules forwarding tests, plus renamed independent no-default consumer. |
| T02 | FAIL | Duplicate/collision UI cases pass; legal raw root identifier fails, F-02. |
| T03 | PASS | `attributes::cfg_and_deprecated` under deny(warnings), disabled missing types/subtree; deprecation UI warning at navigation. |
| T04 | PASS | 51 current-toolchain UI cases in restored relocated copy; native header bound has independent semantic compile failure on MSRV. Original-path harness remains environment-mismatched. |
| T05 | PASS | Standalone no-default and JSON-only consumers plus feature trees; no reliance on workspace dev-feature unification. Documentation exception is F-03. |
| T06 | BLOCKED | HTML/signatures inspected and five doctests compile, but rendered browser QA blocked by unavailable access host; migration example separately fails, F-04. |
| T07 | PASS | Fresh locked source baseline and 102 runtime assertions on current/MSRV toolchains. |
| T08 | BLOCKED | Linux executed. No Windows host/VM/execution receipt; CI definition alone is not Windows evidence. |
| T09 | PASS | Local package lists, production manifests, dependency paths/versions and feature graphs inspected. No full registry-package installation claim. |
| T10 | NOT_RUN | Retained scale generator/A-B scripts inspected; no current before/after measurements or raw timing runs. |
| T11 | PASS | This matrix names actual assertions for scoped PASS claims; historical PASS tables and timings were not inherited. |
| T12 | PASS | Final baseline pass → three assertion-failing mutants → restored pass, with explicit scratch paths and receipts. |

**User-task disposition.** Same verified source identity and evidence scope apply. A source route or deliberate limitation is distinguished from an executed task.

| Task | Status | Route / evidence |
|---|---|---|
| U01 | PASS | Separate renamed/shapes crates; `slice::m01_health_round_trip` and shapes wire tests. |
| U02 | PASS | Bodies suite: JSON read/create, no-output delete, explicit representation types. |
| U03 | PASS | Root arities 0 in minimal fixtures, 1 in values/native, 3 in probe Graph, 8 in Wide; missing config-field UI. |
| U04 | PASS | Named root config reduces positional swap exposure; explicit wrong field assignments remain possible and are documented; distinct wire-spelling newtypes demonstrate a typed route. No human error-rate claim. |
| U05 | PASS | Nested region/tenant reuse and snapshot tests; selected parent handles retained. |
| U06 | PASS | Optional absent/present-empty tests assert distinct query outcomes. |
| U07 | PASS | Nullable payload and nested-absence tests, ordinary Option each-override case. |
| U08 | PASS | Counter/clone/prepare/send retention tests. |
| U09 | PASS | Parent setter and native authorization update tests preserve old children. |
| U10 | PASS | D04 truth table and independent two-branch oracle. |
| U11 | PASS | Repeated collection query and present-empty replacement tests. |
| U12 | PASS | Secret header wire controls, reused URL-placement sensitivity tests and independent encoded key probe. |
| U13 | PASS | Runtime base-prefix tests plus independent escaped/IDNA base case. |
| U14 | PASS | Native problem-document recipe preserves status and decodes unread non-2xx body; non-status error propagates. |
| U15 | PASS | Native timeout/header edit plus retained typed decoder reaches loopback. |
| U16 | PASS | Downstream NDJSON Encode/Decode, independent move-only custom body encoder. |
| U17 | PASS | Actual downstream compile-fail UI cases identify wrong ancestors, duplicates and incompatible values. |
| U18 | PASS | Real renamed consumer and forwarding macro tests. |
| U19 | PASS | Standalone no-default/JSON-only consumers and isolated feature graphs. |
| U20 | NOT_RUN | Generated signatures/prose and quick start inspected; no reader-only learning trial was performed. Visual QA is blocked and the no-default rustdoc link defect is F-03. No human usability result claimed. |
| U21 | PASS | Foreign ID with three explicit newtypes and independent expected spellings. Adapter syntax remains deliberately unsupported. |
| U22 | PASS | Foreign NonZeroU64 ID wrappers implement local Display without orphan-rule violations. |
| U23 | PASS | Inventory declaration uses explicit JSON endpoints, a binary exception, custom formats and no-output endpoints; no scoped default inferred. |
| U24 | PASS | Independent typed application wrapper stores non-wire context. No local-state DSL is required. |
| U25 | PASS | 1 MiB value records zero preparation clones; move-only body/value route and body-bearing Clone test. |
| U26 | PASS | Shared atomic interior mutation is observed by a pre-existing selection at prepare; structural semantics verified. |
| U27 | PASS | cfg/deprecated subtree/navigation tests; visibility follows client visibility as documented, rather than an unpromised endpoint-visibility grammar. |
| U28 | PASS | Actual chunked early drop and application timeout fixtures. |
| U29 | FAIL | Native/custom form route works; multipart parameter backslashes corrupt, F-01. No core multipart subsystem needed. |
| U30 | PASS | Finite cursor and bounded replay-safe application retry examples with explicit assumptions and bounds. |
| U31 | PASS | Application semaphore admission and supplied-key rotation, with wire/admission positive controls. |
| U32 | PASS | Native interleaved query rewrite asserted as an exact loopback query string. Core per-key slots intentionally do not express interleaving. |
| U33 | PASS | Independent escaped/IDNA runtime base; static strict subset, IP/host/header limits and errors exercised. |
| U34 | NOT_RUN | Generator and multiple-consumer harness inspected; no 1,000-endpoint build or fanout/performance comparison personally executed. |
| U35 | NOT_RUN | Syntax→model→emitter→Parts ownership traced in source; no fresh-maintainer editing trial or usability result claimed. |
| U36 | FAIL | Maintained code callers compile, but current migration text omits/contradicts the refined alpha constructor/body surface; F-04. |

**Limitations and triage boundary.** Full original-path `cargo test --workspace` is not green because of the documented registry-path snapshot mismatch. Default strict docs pass; no-default strict docs do not. Windows, rendered browser QA, large sensitivity stress, current performance/codegen/fanout measurements and a fresh-user task-first documentation trial remain unvalidated. The eight independent tests and 1,458 oracle preparations are useful counterexamples, not exhaustive semantics or a usability study. Initial invalid probe/mutation attempts are disclosed above. Detailed personally executed shell commands are retained in [the command ledger](executed-shell-commands.json); reads and historical claims remain distinct from executed tests.

**Scoped verdict: REVISE for F-01 through F-04; coordinator triage remains pending.**
