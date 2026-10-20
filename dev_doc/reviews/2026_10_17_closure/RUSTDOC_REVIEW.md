# Rendered Rustdoc technical review

Review date: 2026-10-17 UTC. This is an AI technical reading of fourteen actual generated pages, using their retained HTML text. It is not a first-contact human review, a browser-navigation test, or a release-wide verdict.

## Result and source identity

The selected public contracts are readable and consistent on the subjects described below. One minor wording error was found in the `IntoValue` cancellation paragraph. The accompanying source change corrects that sentence. A later native check on 57c9ec2 regenerated it successfully, as recorded separately below. No other Critical or Major content finding was identified within this selection. The wider navigation, representation-fence and semantic-diff tasks are limited as stated below.

| Evidence | Identity |
|---|---|
| Source that produced the fourteen reviewed pages | `579e8eac9a436a57ebc5ea3fcc2e7da3cf330c31` |
| Rendered source Git tree | `175b236b4b999bb9773e1f1ddb94948442d7d856` |
| Linux qualification | [Run 38981477842, job 117243098047](https://github.com/gqsnt/concord_api/actions/runs/38981477842/job/117243098047), attempt 1 |
| Original Linux artifact | `12082301060`, `v3-linux-qualification-38981477842-1`, 22,816,017 bytes |
| Original artifact SHA-256 | `23fd807f6eb1a114cd852e961b665765a8835af0fcb1cdfb67e532b239032e23` |
| Readable re-exposure of actual archived output | [Run 38983860434, collector job 117250663206](https://github.com/gqsnt/concord_api/actions/runs/38983860434/job/117250663206), driver `2f647826dfefb61a6e5a6770f8af2c523198011e` |

The collector reports that it downloaded the artifact and verified its provider digest. I read the fourteen `CI_EVIDENCE_RUSTDOC_TEXT` records, their `CI_EVIDENCE_RUSTDOC_PAGES` index, and the three command receipts from its actual log. There were exactly fourteen unique expected paths, all present, with nonempty text and matching 64-character SHA-256 values in the text records and page index. The text contains 145,636 characters in total, including Rustdoc navigation and inherited trait material. The original HTML files remain in the artifact. I did not independently download or recompute their byte hashes in this review.

The prior product snapshot `5b3772236d7c1b71f86ebeba35f7d33a30cf3b64` / tree `dca219853f3fadd94c6c6e69699839a9c8d67514` is related by an independently read GitHub comparison: only five qualification files were added between it and `579e8ea`. This does not promote these receipts to any later commit. In particular, the one source-documentation sentence changed below is not present in these fourteen old HTML hashes.

## Native receipts inspected

All three receipts identify source `579e8eac9a436a57ebc5ea3fcc2e7da3cf330c31`, tree `175b236b4b999bb9773e1f1ddb94948442d7d856`, clean start and end states, unchanged inputs, no timeout, no launcher or end-snapshot error, and an empty digest-error map. The start and end input-manifest SHA-256 values are identical: `6d89f739d0bf0478b8542baf27c45dba6bf7a33bb1892568ac3446cd5dd6bfe8`.

| Command | Native exit | Elapsed seconds | Native log SHA-256 |
|---|---:|---:|---|
| `just verify-implementation` | 0 | 1248.363 | `d45bc6b48b336adecc14a6d97c3776e99b526a3255b135555aab62cb7e8bc57d` |
| `just analyzer` | 0 | 100.452 | `af0920f68eebf78b0367df0f0851e2a171a4663b9b26498e6a464d7fe63a6a0b` |
| `just release-audit` | 0 | 8.976 | `a1eeac8e51dd37315b833975b67429bc320682a637d1ef74fafbdb7cbbba7e6e` |

The receipts record `RUSTUP_TOOLCHAIN=1.97.1`, two Cargo build jobs, disabled incremental compilation, and zero dev/test debug information. `CARGO_NET_OFFLINE` is absent from their selected-environment map; these receipts alone do not establish that setting. I did not execute these commands or infer per-test counts from their aggregate exits. The qualification completed on 2026-10-16 UTC; the later collector re-exposed its preserved results.

## Content actually reviewed

| Subject | What the rendered pages establish |
|---|---|
| Client discovery and construction | `CatalogApi` directs the reader to its builder and names the required tenant and bootstrap secret. The builder explains accumulated missing-input errors by setter name, default evaluation once, immutable snapshots, and no network work during build. The client explicitly requires limiter selection for declared rate policies. |
| Value selection and conversion | The builder, regional scope and paginated request distinguish present payload selection, exact optional-slot replacement, conditional `None` as no new opinion, omission, and reset to a retained initializer. Required, optional and initialized values have distinct method sets. The corrected generated prose explicitly limits borrowed-string conversions to a `String` target. `IntoValue` documents identity plus the two borrowed-string conversions, sealed extension, no widening or parsing, and a negative numeric-widening example. |
| Origin, endpoint contract, authentication and rate policy | Regional endpoint summaries show the concrete logical URL template, method, explicit or navigation-derived endpoint identity, semantic result and representation, accepted statuses, effect, timeout, credential/challenge choice, and rate-policy provenance. The examples distinguish a bootstrap-header login from the regional bearer session. `contract()` and `description()` are explicitly value-free public paths to the detailed contract. |
| Absence, terminals and errors | `FindItemRequest` states that a 404 produces `None` without polling that response body, while its normal accepted statuses remain distinct. Its `.response()` terminal returns metadata with the declared optional value. Generated controls explain timeout replacement versus retained caps, zero/unrepresentable-duration errors, the bound on distinct cancellation signals, and that caps never grant retries. |
| Pagination | `ItemSummariesRequest` explains that ordinary await fetches one page. `pages(u32)` is caller-controlled, `collect_prefix(u32)` may stop at a bound without claiming exhaustion, and `collect_all(NonZeroU32)` requires exhaustion within its bound. `TraversalSummary` distinguishes pages received from delivered, records continuation knowledge and requested/effective bounds, and does not claim that delivered items were processed by the application. |
| Byte streams | `ByteStream` distinguishes `StreamOpened` from body completion; documents one terminal error followed by `None`, actual EOF, early finish/drop, cancellation-safe individual `next` futures, external cancellation, finite total and idle leases, precedence at expiry, no retry after handoff, and value-free correlated error summaries. It explicitly contrasts pending `next` cancellation with pagination fetch cancellation. |
| Record streams | `ItemStream` carries the byte-stream contract into record framing and decoding, states frame/record bounds, explains that transport chunks are not record boundaries, describes final-record policy, and documents cancellation-safe buffering and failure instead of a silently dropped incomplete tail. |
| Explicit metadata authority and omission | `RequestPlan` explains inert shaping, omitted credentials/body bytes/protected rate keys, and that exposed public URL/header values can still contain application data. `ApiResponse` states that its explicit header accessor can return credentials or other sensitive data, while `Debug` and automatic observation omit header values and the logical URL. Stream and traversal summaries explain their value-free scope. |
| User-authored deprecation | The three `LegacyApi` fixture pages retain deprecation annotations on the endpoint, named request and value controls, and identify the retired filter as no longer sent. This is a generated V3 provider-API deprecation fixture; its name is not evidence of a V2 execution adapter. |

## Minor finding and exact source correction

`IntoValue` says that conversion occurs inside a setter or constructor call, “before any request exists”. Generated optional-value setters can operate on an already constructed, inert request. The intended distinction is synchronous selection before execution.

- Path: `crates/concord-core/src/value.rs`, documentation line 54 in parent `2bba4e3797ece2f8254efd84ef31640605b65ff8`.
- Old Git blob: `1b1daf8893537518d4172010ee3e2461868c3908`.
- Replacement Git blob: `a0ccefb77d25ccf26c9210c2152abe1a72c35f52`.
- Exact old line:

```text
///   setter or constructor call, before any request exists.
```

- Exact replacement line:

```text
///   setter or constructor call, before request execution.
```

The substitution occurs exactly once. It changes only a documentation comment; signatures, implementations, tests and runtime behavior are unchanged. The rendered `IntoValue` SHA-256 in the index below intentionally identifies the pre-correction output where the issue was found.

The original fourteen-page review predates the correction. The later native check below finds `before request execution` and rejects `before any request exists` in newly generated HTML. The final integrated return repeats that command on its own source identity.

## Subsequent native correction and Catalog cross-check

[Final-return attempt 1](https://github.com/gqsnt/concord_api/actions/runs/38987720123/job/117262964823)
ran on `57c9ec273464dfddee1d91052e86f37b939e4c0f`. Its native
`env RUSTDOCFLAGS=-D warnings cargo doc --locked -p concord-core --all-features --no-deps`
command exited 0. The command log SHA-256 is
`37980a7419acea12613004f1b95b9ade0bc7f8f00319368e6423f720e9112127`.
The generated IntoValue HTML SHA-256 is
`545fac65362cc4138d76757db1160b678fb46bc91f968a5bb56d87084a0a239e`;
its extracted text SHA-256 is
`aa140caae35fb1ed5602ca14ad9afd74871d9254170e1923dc61e837027aa613`.
The native receipt records the new sentence present and the old sentence
absent. This is a regenerated-output check and receipt reading, not a new
visual or browser-navigation review.

All 24 native CLI controls in that attempt matched their expected results.
The actual Catalog export (`concord.contract/3`, SHA-256
`6565846a07e8490e44ca2989f4b54ffaf5db3e98dbdc76563e137ceb43e00b33`)
was cross-read against its unchanged declaration, generated callers and the
retained endpoint cards for `catalog.item.find` and `catalog.items.list`.
The selected records agree on safe GET, explicit endpoint identities,
20-second inherited client timeout, buffered Json, `Option<Item>` plus
404 absence for find, and `Vec<ItemSummary>` plus OffsetPagination for list.
They also agree on the bearer session replacing the bootstrap header,
absence of automatic recovery for that source, tenant sensitivity and
cost-one tenant quotas of 120/minute and 5,000/hour. The find export's
`{endpoint.id}` and card's `{id}` refer to the same endpoint-owned route hole.
The reduced JSON sample does not contain every owner value; the initial
0/25 slots and complete setter matrix are corroborated by their declaration,
HTML and maintained assertions, not by fields absent from that sample.

That return still failed overall: 31 of 33 command expectations passed,
with the envelope-test helper import and two review-report Rust fences failing.
Its positive Rustdoc/CLI receipts do not imply a successful final assembly.
Both delivery defects are corrected without changing the library or those
CLI/UI blobs. The original failed artifact and its source-bound native logs
are retained by the next final return.

## Coverage limits

- The reader used text extracted from actual HTML with scripts and styles omitted. Links, anchors, search, keyboard navigation, visual layout, accessibility and browser rendering were not exercised. The inherited-trait boilerplate was not treated as the product contract.
- Detailed request/response representation provenance, independence of the two inheritance directions, every explicit fence, and the complete affected-descendant set are not all spelled out in these fourteen pages. The public `contract()`/`description()` paths are visible, but the initial fourteen-page pass did not traverse every linked description type. The subsequent two-endpoint export cross-check above is deliberately bounded and does not claim every representation fence or affected-descendant case.
- The initial fourteen-page pass executed no Catalog `export`, `check` or `diff` command. The subsequent real CLI checks have separate 57c9ec2 receipts; no prior local command result is promoted to that source.
- The review checks explanatory content, not runtime security by reading documentation alone. Source-based and native behavioral qualifications remain separate evidence.
- The reviewer is an AI agent familiar with the implementation. No human reviewer, timing, confidence questionnaire, independent first-contact session, or public-freeze usability result is invented. The optional human review remains unexecuted.
- The three native receipts establish their exact commands and source. They do not by themselves establish a complete final package, every later source change, or a universal absence of defects.

## Reviewed HTML index

Paths are relative to the qualification Cargo target directory. Hashes identify the original HTML, not the extracted text.

| # | Generated HTML path | SHA-256 |
|---:|---|---|
| 1 | `doc/concord_examples/contracts/catalog_v3/struct.CatalogApi.html` | `ec1c512ae819ac500fe246bd32155770fe345ffd6a8b8c227e7472cd76ebd8f7` |
| 2 | `doc/concord_examples/contracts/catalog_v3/catalog_api/struct.CatalogApiBuilder.html` | `e08fea59281bc88dbde975ff8d259a7877a0b01cf60d44bcf7a05d15133ab20f` |
| 3 | `doc/concord_examples/contracts/catalog_v3/catalog_api/regional/struct.Regional.html` | `84ea1e057f41887856b71da872ee8970a323b2070c1013c50b552b7f4fa9d456` |
| 4 | `doc/concord_examples/contracts/catalog_v3/catalog_api/regional/struct.FindItemRequest.html` | `f57eaa96391b32897a5b382b1eab17990f6d8cc71c3b1a8bdd90bdaa81eaae31` |
| 5 | `doc/concord_examples/contracts/catalog_v3/catalog_api/regional/struct.ItemSummariesRequest.html` | `5d665ff24fd85310611c2b80f6c6c118db81858d957e79d7b1041bf8dac7d214` |
| 6 | `doc-audit/doc/concord_doc_audit_fixture/struct.LegacyApi.html` | `53f7b1a07abf15fb262b61005cdb1c4df167624c79724277138239ea7c20b938` |
| 7 | `doc-audit/doc/concord_doc_audit_fixture/legacy_api/struct.ExportRequest.html` | `d057d23ce08d7824f63f713948b7d532cd04bab3b32ae5f56ca0462290e82cba` |
| 8 | `doc-audit/doc/concord_doc_audit_fixture/legacy_api/struct.LegacyApiBuilder.html` | `65b2db34881a71150d4546a917e4912b8f1ea0893de2c0bb378e74b2900d9bd1` |
| 9 | `doc/concord_core/contracts/trait.IntoValue.html` | `f9d62687cbf09aa8fd5b87ef0de5b3dcf985a7346b6219b9d840b010fcbcf455` |
| 10 | `doc/concord_core/contracts/struct.RequestPlan.html` | `82b5af5002294be6e6dc784cd170dac83203b11740dcc9c96bcb15f71453936a` |
| 11 | `doc/concord_core/contracts/struct.ApiResponse.html` | `bd8ab8ab63fb5f3676e4d8142cc3064ea213f72478fb81eb4148e1ab75374375` |
| 12 | `doc/concord_core/contracts/struct.ByteStream.html` | `40edebe195d89358d23f127b8a538b26a496800185751b4fe6fd8d3aa416672e` |
| 13 | `doc/concord_core/records/struct.ItemStream.html` | `7fd30c6f84b86cc3776a9ec54b0d4f5f8912485b3377347a4738e099d94ed557` |
| 14 | `doc/concord_core/pagination/struct.TraversalSummary.html` | `e901bc7a54f26f2d62da09fbe99e0d1d1ae33b35a8cdca95a4ecc09ddaf400a3` |
