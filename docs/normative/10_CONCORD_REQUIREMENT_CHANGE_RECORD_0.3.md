# CONCORD_REQUIREMENT_CHANGE_RECORD_0.3

**Version:** 0.3
**Status:** Normative Implementation Candidate
**Authority:** Phase 12 requirement lineage for generic body/response specifications and integrated records
**Depends on:** Specification Index, Product Contract, Requirement Change Record 0.2, owning normative documents
**Supersedes:** None

## 1. Purpose and authority

This record establishes exact requirement lineage for the user-authorized
`CONCORD_PHASE_MISSION` dated 2026-07-20:

`Phase 12 — Generic Body/Response Specifications and Integrated Streaming Records`.

For every row below:

- the implementation phase is Phase 12;
- authorization is the user-approved Phase 12 directive;
- a retained disposition means the identifier remains active with an explicit paragraph-version
  change and the replacement identifier is the same identifier;
- a superseded disposition removes the prior identifier from current active coverage while keeping
  it permanently resolvable through this record;
- a new disposition has no prior paragraph or prior digest;
- historical evidence remains valid only for the exact prior requirement paragraph, requirement
  version, and immutable source identity recorded by that evidence;
- historical evidence is not rewritten, reclassified, or treated as proof of the Phase 12 paragraph;
- current Phase 12 evidence must name the active paragraph and final immutable Phase 12 source.

Prior paragraph digests are lowercase SHA-256 of the Phase 11 paragraph at starting commit
`b66d2914609aec7b767481e09dedd5544b3d9130`, normalized by trimming and replacing each whitespace
run with one ASCII space. The normative owner column identifies the document owning both the prior
paragraph and its current replacement unless a superseded row states another replacement ID.

## Retained identifiers with explicit version changes

Source-grounded review found every mission-listed paragraph materially changes. The same audit found
three additional materially changed paragraphs: `PROD-GUAR-009`, `DSL-END-001`, and
`EXEC-DEADLINE-001`. Most changes replace macro/private semantic categories with public type-owned
specifications; the remainder explicitly extends diagnostics, finite limits, features, conformance,
or lifecycle wording to `ApiResponse`, `ByteStream`, generic specs, prepared endpoints, and
integrated records. No reviewed paragraph is classified unchanged.

| Prior ID | Prior title | Prior paragraph digest | Normative owner | Phase 12 disposition / replacement |
|---|---|---|---|---|
| `PROD-CORE-002` | Typed endpoint results | `2bb80810d45fa82eec1c94bc84a738aa4c58c39655ceb464055d4c41b4d7c2d8` | `01_CONCORD_PRODUCT_CONTRACT_0.1.md` | retained with explicit version change as `PROD-CORE-002` |
| `PROD-CORE-003` | One execution runtime | `f54d287a64e0bf7145fe1c59af9edd1ccb3b5055d7a40aad044328c3fb5d9081` | `01_CONCORD_PRODUCT_CONTRACT_0.1.md` | retained with explicit version change as `PROD-CORE-003` |
| `PROD-GUAR-002` | Body-free automatic diagnostics | `7c0bd060b40b6d60433639acbf2d5c4a93998b9004297cf44d188b8e22525682` | `01_CONCORD_PRODUCT_CONTRACT_0.1.md` | retained with explicit version change as `PROD-GUAR-002` |
| `PROD-GUAR-004` | Bounded ordinary I/O | `888e4de891cad8a40689627398307ed66c94fcb128f98fd6b527d938d8cf7e51` | `01_CONCORD_PRODUCT_CONTRACT_0.1.md` | retained with explicit version change as `PROD-GUAR-004` |
| `PROD-GUAR-008` | Hidden retry containment | `7d6081acaa7b2f87a5dceffe70cdaefce80e417ff7242e147d2b9c6a2160e24b` | `01_CONCORD_PRODUCT_CONTRACT_0.1.md` | retained with explicit version change as `PROD-GUAR-008` |
| `PROD-GUAR-009` | Provider isolation | `53f21b4c5cc8449f267a556e3a5d83a9360b796dbcde15bb190aa2ec8783624a` | `01_CONCORD_PRODUCT_CONTRACT_0.1.md` | retained with explicit version change as `PROD-GUAR-009` |
| `PROD-GUAR-010` | Fail closed | `42bd893e04deb8853e60c6c3192f9d6c55d2c88e31fc1f701bd140e7342ceea1` | `01_CONCORD_PRODUCT_CONTRACT_0.1.md` | retained with explicit version change as `PROD-GUAR-010` |
| `DSL-BODY-001` | No-content request marker | `1d46c4345372bf2a577859f9540f2e2d6384fab4758198f9396be6e7b6e8d9aa` | `02_CONCORD_DSL_SPEC_0.1.md` | retained with explicit version change as `DSL-BODY-001` |
| `DSL-BODY-002` | Multipart request only | `17ab148983a5b046fb43350f3dbbdc380424344941b52cc0d327d16974964d86` | `02_CONCORD_DSL_SPEC_0.1.md` | retained with explicit version change as `DSL-BODY-002` |
| `DSL-END-001` | Typed response header | `642e48f471966b19c03de35393f7fe86d16b8e634b62c66536bd1ceeaf6720d1` | `02_CONCORD_DSL_SPEC_0.1.md` | retained with explicit version change as `DSL-END-001` |
| `DSL-RESP-001` | Response compatibility | `ca9a04051a0fcb8b3377d7429a4475859f9fe2753d2de1933931a31708b0e63c` | `02_CONCORD_DSL_SPEC_0.1.md` | retained with explicit version change as `DSL-RESP-001` |
| `DSL-FEAT-001` | JSON gate | `fd5113a56a07e4288cf3443dace894953e6ab43a502a9fc57e0db854c782ccd0` | `02_CONCORD_DSL_SPEC_0.1.md` | retained with explicit version change as `DSL-FEAT-001` |
| `DSL-FEAT-002` | Multipart gate | `722fd09fe583d3ad6cf620f998817d701e18bdc4d698578ef0bb65640424b0be` | `02_CONCORD_DSL_SPEC_0.1.md` | retained with explicit version change as `DSL-FEAT-002` |
| `API-CODEC-001` | Bounded decoder input | `4f3be31b53e7aff2f38714af139053bb8557a7f71a4a207d9201e142a93eade5` | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` | retained with explicit version change as `API-CODEC-001` |
| `API-CODEC-002` | No request metadata privilege | `35227878e689dba9361a5067b993dfd021baee856147e2b19e787db0261fe03c` | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` | retained with explicit version change as `API-CODEC-002` |
| `API-CODEC-003` | Media header ownership | `d6d6f727cea40428a1217ddf36241259b2f85f84cd22fea0c18675357bbd5e05` | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` | retained with explicit version change as `API-CODEC-003` |
| `API-CODEC-004` | Stream media contract | `9b8c7ea993dd3d066c56c122a7f51c70d4a50f9ccff747f8e224221c81e432eb` | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` | retained with explicit version change as `API-CODEC-004` |
| `API-BODY-001` | Internal replayability | `e7289a5a531375fa31a3346625ba3c9adfb19ce3a542ad132f31e4a9d95db249` | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` | retained with explicit version change as `API-BODY-001` |
| `API-BODY-002` | Factory metadata and freshness | `75e6c373716e297cd004e83b748a0960a4213360ed04c249a3e4756a9fa7113c` | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` | retained with explicit version change as `API-BODY-002` |
| `API-BODY-003` | Extension error construction | `50fff04e7820bb1e0e75f23c2d90c447315a86f86c6cff791138ac4ae3b2d38a` | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` | retained with explicit version change as `API-BODY-003` |
| `API-RESP-001` | Successful accessor boundary | `500d8e5cc637db594753a9941794a255f421a0e35b4fa60eb540e6c1c2fbc73b` | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` | retained with explicit version change as `API-RESP-001` |
| `API-PREP-001` | Runtime containment | `f42e223f7a82523e803dfc6d4068b4ea7c5b62526f505a8fc1e1e8e81effd504` | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` | retained with explicit version change as `API-PREP-001` |
| `API-PREP-003` | Deterministic builder composition | `f833f104441836ce347db200f9de4f514355333ad955e430b15f6b58ea7e9c9c` | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` | retained with explicit version change as `API-PREP-003` |
| `GEN-END-001` | Required argument order | `f159410e22b872e386c58355a678dc130b6c9839a065d836251e5590d181b506` | `04_CONCORD_GENERATED_API_SPEC_0.1.md` | retained with explicit version change as `GEN-END-001` |
| `GEN-END-002` | Request creation only | `87eb7f8031e1b6aaa674ac73a86b4c186bba0f3ebca54ceb9188824d5d852399` | `04_CONCORD_GENERATED_API_SPEC_0.1.md` | retained with explicit version change as `GEN-END-002` |
| `GEN-TERM-001` | Direct semantic output | `6fd5ed3e1750d4e1d5afe8a7008a7eb99f12e85270c17b3c3f6dcc0cb7970f50` | `04_CONCORD_GENERATED_API_SPEC_0.1.md` | retained with explicit version change as `GEN-TERM-001` |
| `GEN-TERM-002` | Metadata availability | `ed711f46606dd4f0ddc85d0309397ea1c4deedb558acf8780f6ab5f34e225c41` | `04_CONCORD_GENERATED_API_SPEC_0.1.md` | retained with explicit version change as `GEN-TERM-002` |
| `GEN-PAGE-003` | Conditional availability | `9b0249bfb68b6313970bfebabf0ed4768174970a267eecdf913abdebcb9e669c` | `04_CONCORD_GENERATED_API_SPEC_0.1.md` | retained with explicit version change as `GEN-PAGE-003` |
| `GEN-PAGE-004` | Associated typed controller integration | `42cba96afb4d0af16066d862efa9cac5d9b1acc56dad7593e78a9523ff3d924b` | `04_CONCORD_GENERATED_API_SPEC_0.1.md` | retained with explicit version change as `GEN-PAGE-004` |
| `SEC-BODY-001` | No request body diagnostics | `afd923e51cca29a4dddc95c401684023b8a9d2f6fe203adff4efde6c18a978e7` | `05_CONCORD_SECURITY_AND_LIMITS_SPEC_0.1.md` | retained with explicit version change as `SEC-BODY-001` |
| `SEC-BODY-002` | Request body diagnostic exclusion | `96d85ac4b589b11aa95c54e43290b7185db324b286b5bc8395f8687df4b0ba97` | `05_CONCORD_SECURITY_AND_LIMITS_SPEC_0.1.md` | retained with explicit version change as `SEC-BODY-002` |
| `SEC-RESP-003` | Response body diagnostic exclusion | `df55a4aaf0b0d0dbe548a0f5c09da60d939d166c9e5de2c9b57cd650a392bd8f` | `05_CONCORD_SECURITY_AND_LIMITS_SPEC_0.1.md` | retained with explicit version change as `SEC-RESP-003` |
| `SEC-RESP-006` | Structural raw-response debugging | `cf0de497030912ae5519a27091f40694a63adb54bf2f6658ddd87ce2ab992839` | `05_CONCORD_SECURITY_AND_LIMITS_SPEC_0.1.md` | retained with explicit version change as `SEC-RESP-006` |
| `SEC-RESP-007` | No truncated body exposure | `9a432b6ba30d0ef84e1678e2f99ddaf41344f7b6bcaf598d45bb0b9b6058db1e` | `05_CONCORD_SECURITY_AND_LIMITS_SPEC_0.1.md` | retained with explicit version change as `SEC-RESP-007` |
| `SEC-RESP-008` | Terminal status preservation | `e1b60b0a55682b1d34eb08b1394a158fb547ab5af8891cfa40ed4e695b44d808` | `05_CONCORD_SECURITY_AND_LIMITS_SPEC_0.1.md` | retained with explicit version change as `SEC-RESP-008` |
| `BOUND-BODY-001` | Request default | `cb9e74f7184dfc56e132d94d6e042fa14d9a034053226313770091bbebe604b8` | `05_CONCORD_SECURITY_AND_LIMITS_SPEC_0.1.md` | retained with explicit version change as `BOUND-BODY-001` |
| `BOUND-BODY-002` | Buffered response default | `bfffd2a5d6b3a24e90f401564a4f91db6db7d93cb3e3fa4622ec99663e8b75f0` | `05_CONCORD_SECURITY_AND_LIMITS_SPEC_0.1.md` | retained with explicit version change as `BOUND-BODY-002` |
| `BOUND-BODY-003` | Stream response default | `0203836a40ee024c1448b081463b869f21dc9e8fa37342ae5d29c389ad86845f` | `05_CONCORD_SECURITY_AND_LIMITS_SPEC_0.1.md` | retained with explicit version change as `BOUND-BODY-003` |
| `BOUND-BODY-006` | Excess-frame withholding | `47df87d248df09cc3559b64f7614d83da57b2bc2f5e84c126968996e7f426291` | `05_CONCORD_SECURITY_AND_LIMITS_SPEC_0.1.md` | retained with explicit version change as `BOUND-BODY-006` |
| `BOUND-BODY-007` | Finite ordinary limits | `6b669dd6aec06524e9e1d3fda2a737442f53eddd033faa6da5b2f855ccc13ef2` | `05_CONCORD_SECURITY_AND_LIMITS_SPEC_0.1.md` | retained with explicit version change as `BOUND-BODY-007` |
| `BOUND-BODY-011` | Post-decompression response accounting | `1e61a77dd709a0361cbb27d77850c3aa7710ff8278973c78214536cd279bd46a` | `05_CONCORD_SECURITY_AND_LIMITS_SPEC_0.1.md` | retained with explicit version change as `BOUND-BODY-011` |
| `EXEC-BODYMETA-001` | No eager materialization | `5c92b89b23a18db35fc6b7d0eb7548c646fdb3bbae2e4888c63b5ad12a8f59cd` | `06_CONCORD_EXECUTION_SPEC_0.1.md` | retained with explicit version change as `EXEC-BODYMETA-001` |
| `EXEC-BODY-001` | Body before provider and quota | `8d831d52f27ae78da87ef1ae22cc8ceb785e5218490023e704eb6da7f7d5d6bd` | `06_CONCORD_EXECUTION_SPEC_0.1.md` | retained with explicit version change as `EXEC-BODY-001` |
| `EXEC-BODY-002` | Codec once | `61e1602eecd02056e0461d22599134cbe0d2de76547c77730f328ee6af559b86` | `06_CONCORD_EXECUTION_SPEC_0.1.md` | retained with explicit version change as `EXEC-BODY-002` |
| `EXEC-BODY-003` | Factory per Concord attempt | `37d3f4586d803f9de382bea8452d1032251050d2aab9a3e047aaeef92f922cc6` | `06_CONCORD_EXECUTION_SPEC_0.1.md` | retained with explicit version change as `EXEC-BODY-003` |
| `EXEC-BODY-004` | Stream laziness | `c4e3b39db5f396bd2e1f28170171fb23274ed8fb5387372b0d1a1b0d93e22ca4` | `06_CONCORD_EXECUTION_SPEC_0.1.md` | retained with explicit version change as `EXEC-BODY-004` |
| `EXEC-BODY-005` | Exact-length enforcement | `7c1c70db79f9165c37c7f95e4618aaa1621a2b7a74a0ddb57cbd77889b917048` | `06_CONCORD_EXECUTION_SPEC_0.1.md` | retained with explicit version change as `EXEC-BODY-005` |
| `EXEC-BUFFER-001` | Known excess preflight | `f10680d534fe3e981405ce271092377aa4149d86079831b07972a85fd94a7d13` | `06_CONCORD_EXECUTION_SPEC_0.1.md` | retained with explicit version change as `EXEC-BUFFER-001` |
| `EXEC-BUFFER-002` | Excess frame withholding | `71762233b9f8fc05090213808836637827582f8f6ded15b2daa48a741a61a0de` | `06_CONCORD_EXECUTION_SPEC_0.1.md` | retained with explicit version change as `EXEC-BUFFER-002` |
| `EXEC-BUFFER-003` | Complete body before decode | `68a39953d5f9ad29b2ec0d98e55ae88b55c9a3a549e64ec911ac58f7d190947b` | `06_CONCORD_EXECUTION_SPEC_0.1.md` | retained with explicit version change as `EXEC-BUFFER-003` |
| `EXEC-NOCONTENT-001` | Ignore successful body | `4271c3599f917b6b61f3ef92f5ce5378d714fdaba94c1b1c576f12e026f6f57b` | `06_CONCORD_EXECUTION_SPEC_0.1.md` | retained with explicit version change as `EXEC-NOCONTENT-001` |
| `EXEC-STREAM-001` | Lazy unknown body | `500fffe7b2cebd866ed82804e5c203113bfe949507285a44edeeb14ee038d0af` | `06_CONCORD_EXECUTION_SPEC_0.1.md` | retained with explicit version change as `EXEC-STREAM-001` |
| `EXEC-STREAM-002` | Chunk bound | `81e972f5611822bf78fa4d6dfbd6bae7947f5fa7fced6d479df74a827ae7847b` | `06_CONCORD_EXECUTION_SPEC_0.1.md` | retained with explicit version change as `EXEC-STREAM-002` |
| `EXEC-STREAM-003` | Stream-open outcome | `2edb007546b9337f929897a98593b689b911486272296ed0204e8b87d79bd8d6` | `06_CONCORD_EXECUTION_SPEC_0.1.md` | retained with explicit version change as `EXEC-STREAM-003` |
| `EXEC-DEADLINE-005` | Returned stream boundary | `5e355072326f4a292ef7d0bd23858150354e1208c24b4475111e88a6b131e04b` | `06_CONCORD_EXECUTION_SPEC_0.1.md` | retained with explicit version change as `EXEC-DEADLINE-005` |
| `EXEC-DEADLINE-001` | Operation-wide result deadline | `bfae87f0d0559ef7649dee0c6ae8968ae824f4a540cc778389515f154c71caf4` | `06_CONCORD_EXECUTION_SPEC_0.1.md` | retained with explicit version change as `EXEC-DEADLINE-001` |
| `EXEC-COMPLETE-003` | Stream-open semantics | `2b812000b4232970d579a2deec126882caf9d84264b32fd95ed906ac3b1dfa7e` | `06_CONCORD_EXECUTION_SPEC_0.1.md` | retained with explicit version change as `EXEC-COMPLETE-003` |
| `EXEC-RAW-001` | Runtime parity | `a84b2bfc6491b42b7cb0540e9bc4dfaef50b56970b02286ac723cffaaed7b1ce` | `06_CONCORD_EXECUTION_SPEC_0.1.md` | retained with explicit version change as `EXEC-RAW-001` |
| `FEAT-JSON-001` | Independent JSON | `51e71c4239033d48ca7839f4402cbd244b45d22993dd2058f7e9c0ad22cfd920` | `07_CONCORD_FEATURE_AND_COMPATIBILITY_SPEC_0.1.md` | retained with explicit version change as `FEAT-JSON-001` |
| `FEAT-MULTIPART-001` | Request-only multipart | `dc735461b6aa4f95642d8e0f56ff91e41497aab62edfff280ae6a2a87b6d54e8` | `07_CONCORD_FEATURE_AND_COMPATIBILITY_SPEC_0.1.md` | retained with explicit version change as `FEAT-MULTIPART-001` |
| `FEAT-COMPRESS-001` | Independent decompression | `0fc4df401f831ae5a42552c0eece737816024b7491c1a2e1c9171eccf7d5a94d` | `07_CONCORD_FEATURE_AND_COMPATIBILITY_SPEC_0.1.md` | retained with explicit version change as `FEAT-COMPRESS-001` |
| `COMPAT-MACRO-001` | Single feature authority | `828cbfaaa82e08d9de9594d6c11053403703ca57f99176076fd99ef45722d8ee` | `07_CONCORD_FEATURE_AND_COMPATIBILITY_SPEC_0.1.md` | retained with explicit version change as `COMPAT-MACRO-001` |
| `COMPAT-SURF-001` | Public extension sufficiency | `63a1ff64697b0c0643af61ad352919d67f1962519a8d02334917cf69c036495c` | `07_CONCORD_FEATURE_AND_COMPATIBILITY_SPEC_0.1.md` | retained with explicit version change as `COMPAT-SURF-001` |
| `COMPAT-BUILD-001` | MSRV evidence | `94489cc1e5911af7e6f7eb3fd8b5384133ec490dffc4a72c8a689db740cf42ff` | `07_CONCORD_FEATURE_AND_COMPATIBILITY_SPEC_0.1.md` | retained with explicit version change as `COMPAT-BUILD-001` |
| `CONF-COMPILE-001` | Canonical generated use | `039a13d309ff61f62b004acd756d6a23da539af3f55b56281470cadacf3cbf3a` | `08_CONCORD_CONFORMANCE_PLAN_0.1.md` | retained with explicit version change as `CONF-COMPILE-001` |
| `CONF-COMPILE-002` | Advanced extension use | `e928f1384bd8a2bbf97ca47f5746f63944c694e1cca1a5049f1268d6e91ffff9` | `08_CONCORD_CONFORMANCE_PLAN_0.1.md` | retained with explicit version change as `CONF-COMPILE-002` |
| `CONF-COMPILE-003` | Diagnostic fixtures | `3e29c0f64fffdec9716ee79231f588f958d66bf498a0eee966c049a46288c731` | `08_CONCORD_CONFORMANCE_PLAN_0.1.md` | retained with explicit version change as `CONF-COMPILE-003` |
| `CONF-DOWNSTREAM-001` | Generated public sufficiency | `7ea67551c1f8740f6756c2f2ded55956bef5061086fe911e594f6ced3850b56f` | `08_CONCORD_CONFORMANCE_PLAN_0.1.md` | retained with explicit version change as `CONF-DOWNSTREAM-001` |
| `CONF-DOWNSTREAM-002` | Extension surface sufficiency | `c755da1940405640155922640d46818b79574bec4e4a535b3df4ed170fc77e96` | `08_CONCORD_CONFORMANCE_PLAN_0.1.md` | retained with explicit version change as `CONF-DOWNSTREAM-002` |
| `CONF-SEC-001` | Source-chain scan | `2256f15065d0f899cc8cfc8c2359027a6643ae3d380df3c919db8689821abcbf` | `08_CONCORD_CONFORMANCE_PLAN_0.1.md` | retained with explicit version change as `CONF-SEC-001` |
| `CONF-MSRV-001` | Public MSRV | `fe1037a8036fca3f21f37756a37011091206c0995239c185ca98c82217bb3edf` | `08_CONCORD_CONFORMANCE_PLAN_0.1.md` | retained with explicit version change as `CONF-MSRV-001` |
| `CONF-DOC-001` | No uncompiled canonical example | `7d2123c335f6b90033877b3be8aef9763b4a70837f3adf5d6b83e770b0714ae4` | `08_CONCORD_CONFORMANCE_PLAN_0.1.md` | retained with explicit version change as `CONF-DOC-001` |

## Superseded identifiers

| Prior ID | Prior title | Prior paragraph digest | Normative owner | Disposition | Replacement ID |
|---|---|---|---|---|---|
| `DSL-RESP-002` | No pagination for streams | `166324655d17d7ccd43854480cc7d0f7fc0702cacd0ff21443a894438c15b625` | `02_CONCORD_DSL_SPEC_0.1.md` | superseded | `DSL-RESP-004` |
| `SEC-RESP-004` | Structural decoded-response debugging | `2767e1af6cad90922f87645e579aa61a555264ae97dc597afab9d3ce050fddfe` | `05_CONCORD_SECURITY_AND_LIMITS_SPEC_0.1.md` | superseded | `SEC-RESP-009` |
| `SEC-RESP-005` | Non-consuming stream debugging | `6acf11a9104cd624ababcf49da546122b459ca5eb17ef08053b636b86bbd0828` | `05_CONCORD_SECURITY_AND_LIMITS_SPEC_0.1.md` | superseded | `SEC-STREAM-001` |

## New Phase 12 active identifiers

Every row in this section has disposition `new`, has no prior requirement ID or prior paragraph
digest, and is owned by the named current normative document.

| ID | Exact title | Normative owner |
|---|---|---|
| `PROD-MODEL-008` | Type-owned body and response semantics | `01_CONCORD_PRODUCT_CONTRACT_0.1.md` |
| `PROD-GUAR-014` | Bounded record staging | `01_CONCORD_PRODUCT_CONTRACT_0.1.md` |
| `DSL-BODY-003` | Arbitrary request specification type | `02_CONCORD_DSL_SPEC_0.1.md` |
| `DSL-RESP-003` | Arbitrary response specification type | `02_CONCORD_DSL_SPEC_0.1.md` |
| `DSL-RESP-004` | Capability-gated pagination response | `02_CONCORD_DSL_SPEC_0.1.md` |
| `DSL-FEAT-004` | Type-owned body and response feature gates | `02_CONCORD_DSL_SPEC_0.1.md` |
| `API-BODYSPEC-001` | Request body specification contract | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` |
| `API-BODYSPEC-002` | Pre-hook body metadata | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` |
| `API-BODYSPEC-003` | Produced-body consistency | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` |
| `API-BODYSPEC-004` | One managed outgoing body | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` |
| `API-RESPSPEC-001` | Response body specification contract | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` |
| `API-RESPSPEC-002` | Sealed acquisition modes | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` |
| `API-RESPSPEC-003` | Response-head validation before polling | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` |
| `API-RESPSPEC-004` | Buffered codec adaptation | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` |
| `API-RESP-002` | Universal successful metadata wrapper | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` |
| `API-STREAM-001` | Media-independent bounded byte stream | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` |
| `API-PAGE-011` | Explicit page-response capability | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` |
| `API-RECORD-001` | Incremental record decoder | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` |
| `API-RECORD-002` | Records response and record stream | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` |
| `API-RECORD-003` | Record batching adapter | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` |
| `API-RECORD-004` | Typed CSV decoder | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` |
| `API-RECORD-005` | Typed NDJSON decoder | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` |
| `API-RECORD-006` | Safe record errors | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` |
| `API-PREP-004` | Prepared generic specification selection | `03_CONCORD_PUBLIC_API_SPEC_0.1.md` |
| `GEN-BODY-001` | Type-shaped request-body generation | `04_CONCORD_GENERATED_API_SPEC_0.1.md` |
| `GEN-RESP-001` | Type-shaped response generation | `04_CONCORD_GENERATED_API_SPEC_0.1.md` |
| `GEN-RESP-002` | Universal metadata terminal | `04_CONCORD_GENERATED_API_SPEC_0.1.md` |
| `GEN-PAGE-005` | Capability-based page response | `04_CONCORD_GENERATED_API_SPEC_0.1.md` |
| `GEN-PREP-001` | Prepared generic body and response specifications | `04_CONCORD_GENERATED_API_SPEC_0.1.md` |
| `SEC-BODY-003` | Request-specification extension isolation | `05_CONCORD_SECURITY_AND_LIMITS_SPEC_0.1.md` |
| `SEC-RESP-009` | Structural ApiResponse debugging | `05_CONCORD_SECURITY_AND_LIMITS_SPEC_0.1.md` |
| `SEC-STREAM-001` | ByteStream diagnostic exclusion | `05_CONCORD_SECURITY_AND_LIMITS_SPEC_0.1.md` |
| `SEC-RECORD-001` | Record-data diagnostic exclusion | `05_CONCORD_SECURITY_AND_LIMITS_SPEC_0.1.md` |
| `SEC-RECORD-002` | Record-parser error sanitization | `05_CONCORD_SECURITY_AND_LIMITS_SPEC_0.1.md` |
| `BOUND-RECORD-001` | Positive finite decoder staging | `05_CONCORD_SECURITY_AND_LIMITS_SPEC_0.1.md` |
| `BOUND-RECORD-002` | Record first-excess withholding | `05_CONCORD_SECURITY_AND_LIMITS_SPEC_0.1.md` |
| `BOUND-RECORD-003` | Built-in record staging default | `05_CONCORD_SECURITY_AND_LIMITS_SPEC_0.1.md` |
| `EXEC-BODY-006` | Metadata before hook | `06_CONCORD_EXECUTION_SPEC_0.1.md` |
| `EXEC-BODY-007` | Metadata and produced-body contradiction | `06_CONCORD_EXECUTION_SPEC_0.1.md` |
| `EXEC-RESP-001` | Sealed response-mode acquisition | `06_CONCORD_EXECUTION_SPEC_0.1.md` |
| `EXEC-RESP-002` | Response-specification validation before polling | `06_CONCORD_EXECUTION_SPEC_0.1.md` |
| `EXEC-STREAM-004` | ByteStream ownership and lazy boundary | `06_CONCORD_EXECUTION_SPEC_0.1.md` |
| `EXEC-RECORD-001` | Record stream opening | `06_CONCORD_EXECUTION_SPEC_0.1.md` |
| `EXEC-RECORD-002` | Chunk-independent record framing | `06_CONCORD_EXECUTION_SPEC_0.1.md` |
| `EXEC-RECORD-003` | Bounded record staging | `06_CONCORD_EXECUTION_SPEC_0.1.md` |
| `EXEC-RECORD-004` | Exact record EOF semantics | `06_CONCORD_EXECUTION_SPEC_0.1.md` |
| `EXEC-RECORD-005` | Record-stream drop cancellation | `06_CONCORD_EXECUTION_SPEC_0.1.md` |
| `FEAT-RECORDS-001` | Optional records base | `07_CONCORD_FEATURE_AND_COMPATIBILITY_SPEC_0.1.md` |
| `FEAT-CSV-001` | Optional CSV isolation | `07_CONCORD_FEATURE_AND_COMPATIBILITY_SPEC_0.1.md` |
| `FEAT-NDJSON-001` | Optional NDJSON isolation | `07_CONCORD_FEATURE_AND_COMPATIBILITY_SPEC_0.1.md` |
| `COMPAT-MACRO-005` | Body and response type neutrality | `07_CONCORD_FEATURE_AND_COMPATIBILITY_SPEC_0.1.md` |
| `CONF-BODY-001` | Downstream request specification without macro modification | `08_CONCORD_CONFORMANCE_PLAN_0.1.md` |
| `CONF-RESP-001` | Downstream response specifications for every sealed mode | `08_CONCORD_CONFORMANCE_PLAN_0.1.md` |
| `CONF-RECORD-001` | Incremental CSV and NDJSON framing matrix | `08_CONCORD_CONFORMANCE_PLAN_0.1.md` |
| `CONF-RECORD-002` | Records feature and MSRV isolation | `08_CONCORD_CONFORMANCE_PLAN_0.1.md` |

## Historical evidence and coverage treatment

The Phase 11 change record and every earlier lineage record remain authoritative for their recorded
versions. All 1,553 Phase 0–11 evidence rows and every Phase 0–11 conformance report remain
byte-identical. Prospective Phase 12 coverage is planning, not passing evidence. After implementation
and validation, current coverage contains every active replacement and new identifier, while the
three Phase 12-superseded identifiers remain separately resolvable with `superseded` status.

No row authorizes Phase 13 lexical rate limits or Phase 14 release-final closure.
