# Concord v2.1 public change matrix

| Area | Supplied v1 | v2.1 target | Consequence |
|---|---|---|---|
| source tree | client then sibling scopes/endpoints | all children inside client | module/navigation/source symmetry |
| client values | `vars` | `values` | clearer retained-fact vocabulary |
| optionality | exact `Option<T>` token recognized | postfix `?`; type opaque | alias transparency; breaking declarations |
| collection repetition | inferred/trait/wrapper behavior | explicit `each` at placement | cardinality visible |
| defaults | `= default` | `#[default]` | explicit trait-default policy |
| optional control | inner `.set_x(T)`, `.clear_x()` | exact `.set_x(Option<T>)`, `.omit_x()`, plus initialized `.reset_x()` | nested optional truth, explicit omission/reset, released `with_x` namespace |
| owner refs | `vars`, `scope`, `ep` | `client`, exact scope name, `endpoint` | no search rebinding |
| scalar composition | `fmt[...]` | `concat [...]` | one bounded atom |
| scalar wire text | implicit `Display` | `RequestAtom` or `via Adapter` | stable protocol role |
| host | `host [...]` replaces labels | `subdomain` prepend/replace/off | composable domain hierarchy |
| query/header | implicit optional/repetition; symbols in candidate | assignment, `when_present`, `append`, `each`, `remove` | word-visible inheritance/cardinality |
| request body | `#[body] body: Json<T>` | `body input: T` + inherited/local representation | exact caller input |
| response | `-> Json<T>` / wrapper towers | `-> T`, representation default or inline `via` | exact awaited output |
| representation | leaf wrapper/spec | client/scope directional default + inline exception/fence | lower repetition, effective provenance required |
| status | hidden in response spec | endpoint `status` finite set/range | reflectable success boundary |
| effect | method/repeatability binary | `safe`, `idempotent`, `non_idempotent` | pagination and retry truth |
| response bytes | universal `.response_bytes()` on modes | endpoint `expose accepted_bytes` + `.accepted_bytes()` | explicit authority/capability |
| streams | decoder/acquisition wrapper types | `ByteStream`, `ItemStream<T>`, `RequestStream<T>` | exact lifecycle without mechanism leakage |
| timeout | optional/off/per-page recreation | one finite absolute operation deadline | bounded truthful operation |
| retry | policy + hidden method/body assumptions | app classifier/backoff/budget under effect/body/dispatch gates | no unsafe amplification |
| server delay | extras parses independently | one core `ServerDelayHint` | no duplicate waiting |
| auth | private source/slot model with leaf clauses | retained model, grouped auth, exact challenge attribution | stronger security and readability |
| rate policies | `by`, windows, implicit local feature fallback | stable IDs/key/cost/`allow`; explicit limiter | truthful enforcement scope |
| rate feedback | observer can fail current result | future-admission non-fallible feedback | response cannot be hidden |
| pagination | response Vec/trait extraction, per-page concerns | safe complete page; controller owns items+continuation | envelope/header support; bounded authority |
| hooks | broad before/response/completion authority | read-only `OperationObserver` + narrow policy owners | no middleware import |
| plan | `SafeRequestPlan` | `RequestPlan` credential-free/redaction-aware | honest disclosure guarantee |
| error | implementation/stage variants and hook errors | stable category/stage/dispatch/IDs + safe diagnostics | caller decision model |
| unbound | flat/explicit endpoint forms | isomorphic `Client::unbound()` tree | same domain language |
| runtime endpoint | dynamic builder under old semantics | full v2.1 patch/representation/status/effect, fallible `try_bind` | same executor, no authority escape |
| public version | 1.0.0 internal v1 | 2.1.0 target | breaking release |
