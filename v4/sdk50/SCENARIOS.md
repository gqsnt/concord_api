# Common-case scenario denominator (v1, fixed before results)

Defined 2026-11-07 before any SDK21–50 result, from the P0 inventory (`P0_INVENTORY.md` §6). This is the
denominator later results are counted against; it is a designed scenario matrix, not a population
or market estimate. Each scenario gets **one primary handling category** per SDK that exercises it:
`D` direct declaration, `H` shared helper (named), `A` provider adapter in the SDK, `N` native escape,
`U` unsupported/excluded, plus the failure scenarios themselves. New scenarios discovered later are
appended as v2+ with their version; v1 rows are never edited or removed.

| ID | Scenario | Existing witnesses at P0 (SDK codes as in P0_INVENTORY §1) |
|---|---|---|
| R01 | Finite unauthenticated JSON object/array with required path values. | NH, PK, FF, WB |
| R02 | Client/scope/endpoint inheritance, override and removal produce the intended request. | GH, ST, KU; `sdk`, `shapes` |
| R03 | Absent, empty, null and defaulted request values remain distinct. | ST patch/form, NO query; `compat` |
| R04 | Repeated query/filter values are encoded exactly once. | GL, AT; `shapes` |
| R05 | JSON request body and typed response share normal generated preparation. | OA, LI, NO |
| R06 | Flat form request omits only absent fields and preserves Unicode/reserved bytes. | TW |
| R07 | Nested bracket form supports arrays, maps and clear-versus-omit semantics. | ST |
| R08 | HTTP status chooses different successful representations. | SN 200/202; GD 308/completion; GH 304; `compat` 200/204 |
| R09 | Head-derived output preserves metadata without treating no-output drain as head-only. | S3, DR, GH; GD explicit empty-body check |
| R10 | An undeclared downstream endpoint reuses SDK context through public boundaries. | GH maintenance extension; `extension`/`sdk`; KU closed-surface gap |
| E01 | Non-2xx provider document becomes a typed SDK failure with authoritative status. | TW, FF, GL, ST, GD, NO, MG, DR, SF, KU |
| E02 | Malformed/HTML/absent error document does not turn HTTP failure into success. | FF, GL, SL, DB, GD, ST, DR |
| E03 | Error-document media rejection is distinguishable from document decoding failure. | TW, FF, GL |
| E04 | Oversized error body stops at a bound and retains failure identity/head. | TW, FF, SL, ST; SF/KU manual loops |
| E05 | Interrupted error-body reading preserves HTTP failure and read-failure evidence. | TW, DB, GD, AT, DR; CH collapses cause |
| E06 | Failure before any response has no fabricated status/metadata. | NH, PK, ST, GL |
| E07 | Failure after a received head retains that head. | Most metadata adapters; CH/SF gaps in §2 |
| E08 | HTTP-200 application failure is classified separately from transport success. | SL, WB, SN; TW failure status remains data |
| E09 | GraphQL partial data, errors-only and null connection retain diagnostics. | LI, SH; LI missing-member distinction |
| E10 | Missing, explicit null, zero and unknown variants follow provider model rules. | PK, LI, WB, NO, MG, NH |
| P01 | Strict Link parsing rejects malformed/conflicting continuation fields. | GH, GL, SN, DR |
| P02 | Body next URL is followed unchanged with destination/path validation. | PK, MG, SF |
| P03 | Opaque cursor/offset token is transmitted without trimming or reconstruction. | SL, DB, AT, NO, LI |
| P04 | Numeric limit/offset pagination remains directly addressable. | PK |
| P05 | Page/index plan is derived from provider metadata and validated. | WB pages; SN partitions |
| P06 | Empty nonterminal page continues rather than falsely exhausting. | PK, S3, SL, DB, GD, WB, NO, MG, KU |
| P07 | Contradictory terminal flag/missing next key fails explicitly. | S3, LI, NO, GD, SH |
| P08 | Immediate repeated continuation is rejected. | S3, GL, LI, GD, MG, DR; five controls |
| P09 | Nonadjacent continuation cycle is detected or explicitly bounded. | NO seen sets; caps elsewhere; DR/PK caller-policy gaps |
| P10 | Continuation rejects mismatched bound context before dispatch. | MG, DB, GH, ST, SF app; partial dimensions documented |
| P11 | Restored raw continuation cannot silently migrate to another query/account. | Gap in SL, LI, GD, AT, WB, NO; negative scenario |
| P12 | Exact terminal-page cap succeeds; cap with work remaining reports boundedness. | `Pages` adapters S3/GL/SL/DB/GD/AT/WB/NO/MG |
| P13 | Failed page preserves delivered prefix and failed/restart identity. | GH, ST, GL, SL, DB, AT, WB, NO, MG |
| P14 | Flattened items preserve or explicitly lose page completion metadata. | GD changes token, MG delta, LI/SL diagnostics, GH side channel |
| C01 | Supplied static credential is placed and redacted consistently. | TW, GL, SL, OA, NT, LI, DB, GD, AT, SN, NO, MG; controls |
| C02 | Basic credentials are validated and correctly Base64 encoded. | TW, CH, DR |
| C03 | Invalid credential/header text fails locally without secret diagnostics. | GL, OA, NT, LI, GD, MG, TW |
| C04 | Custom credential headers cannot leak through cross-origin redirect policy. | GH, GL; `riot`; DR caller-transport obligation |
| C05 | Challenge token acquisition separates resource scopes/cache entries. | DR `Access` pull/push/catalog |
| C06 | Expired or unrepresentable token lifetime produces safe cache behavior. | DR `cached_token`, checked `fetch_token` |
| C07 | Untrusted/malformed token realm causes no credential-bearing token request. | DR `TokenAuth::check_realm` |
| C08 | Second authentication challenge terminates after one replay. | DR `Registry::execute` |
| C09 | Concurrent acquisition/refresh is single-flight and cache-safe. | — general guarantee; DR explicitly permits duplicate fetches |
| C10 | Signature covers final target, headers and payload using client-specific state. | S3 |
| C11 | Credential rotation preserves defined behavior of queued old handles. | `native::Session`, `compat::Refresh`; SDKs mostly reconstruct clients |
| X01 | Preparing/inspecting a request performs no network I/O. | All generated SDK surfaces; native upload probes |
| X02 | Logical dispatch, policy subrequests and physical transport attempts are counted separately. | DR challenge flow; GD/SN transport configuration; general gap |
| X03 | Ambiguous keyed write rebuilds the same request with the same logical key. | ST app `Run::write`/`KeyLedger` |
| X04 | Changed request under an existing idempotency key is rejected/reported. | ST app `request_identity` |
| X05 | Ambiguous unkeyed write is not automatically treated as safe to repeat. | TW, S3, GD, CH, DR |
| X06 | Retry attempts/backoff are bounded and conditioned on semantic replay permission. | `native::retry_bounded`; ST app; no general SDK policy |
| X07 | Rate advice distinguishes absent, repeated, invalid, overflow and date forms. | SL/DB strict seconds; GH/GL/NT weaker; date gap |
| X08 | Admission permit covers the intended response/stream lifetime. | Native finite decode example; caller-consumed stream lease gap |
| X09 | Auth replay, retry and rate admission compose under one operation budget. | — general guarantee; isolated DR/ST/native mechanisms only |
| S01 | Declared SSE opens after head acceptance without reading the body. | OA bare stream; NT typed stream |
| S02 | NDJSON/consecutive JSON/watch records are read incrementally. | NT, CH, KU |
| S03 | Runtime CSV reader uses provider/job dialect and column contract. | CH, SF |
| S04 | Record framing survives arbitrary delimiter/UTF-8/CRLF chunk splits. | OA, NT, CH, KU, SF |
| S05 | Wrong response media/head is rejected before a stalled body is consumed. | NT, CH, GL; native head-stage example |
| S06 | Record/event/total-byte bounds fail without claiming clean completion. | OA, NT, CH, SF; shared readers |
| S07 | Provider failure arriving after valid records retains the delivered prefix. | CH late exception, OA failure event, KU ERROR |
| S08 | Clean EOF is distinguished from provider terminal success. | OA completed event, CH marker, SF count validation, KU reconnect |
| S09 | Complete final record without LF differs from truncated record/frame. | NT NDJSON, SH JSONL; SSE has stricter boundary rule |
| S10 | Recoverable malformed record can yield an error and continue safely. | SH JSONL policy; OA buffered parser; shared Yield policy |
| S11 | Cancelling a pending read/drop preserves defined partial state and stops work. | OA, NT, CH, SF, KU apps |
| S12 | Binary EOF validates expected length/digest before claiming verification. | DR digest/size; S3 length/range; GH digest not verified |
| S13 | Runtime response-format/configuration choice remains consistent with request/job context. | OM JSON/CSV, CH reader union, SN inflation cap, SF dialect |
| U01 | Multipart boundary/media and exact part contents remain consistent. | OA form-data, GD related |
| U02 | Contradictory runtime media fails before dispatch. | `multipart` declared-body example; no additional SDK-specific negative witness |
| U03 | Lazy one-shot upload transfers ownership without manufacturing replayability. | CH; `native::OneShot` |
| U04 | Upload-source failure reports possible partial remote effects. | CH; native streamed upload |
| U05 | Known-length-only upload consumes a bounded spool before dispatch. | SF |
| U06 | Preparation/Debug/inspection never polls a lazy upload source. | Native upload source; CH writer-based path |
| U07 | Lost resumable-write response blocks further writes until authoritative probe. | GD `needs_probe`; DR `needs_status` |
| U08 | Resume uses acknowledged prefix, including partial/unaligned acknowledgement. | GD maintenance; DR `BlobUpload::status` |
| U09 | Repeated/regressed/overflowing/impossible upload acknowledgement fails closed. | GD Range checks; DR range/location protocol |
| U10 | Upload completion validates provider status/location/digest rather than request EOF. | GD `UploadSession::accept`; DR `BlobUpload::complete` |
| M01 | Additive unknown fields/enum values preserve compatible existing callers. | NH, TW, PK, NO, MG |
| M02 | Selected nullable GraphQL field change remains aligned across documents/models. | LI dueDate maintenance; SH selection/type independent-knob gap |
| M03 | Provider field/column rename does not require unrelated adapter changes. | AT dynamic fields; OM selected-column mapping |
| M04 | CSV ending inside quoted field is rejected on finite and incremental paths. | OM reproduction; shared CSV scanner |
| M05 | Encoded and inflated response limits are independent and checked. | SN local gzip adapter |
| M06 | Already-owned first page/partition is reused rather than fetched again. | SN partition zero; ST embedded invoice lines |
| M07 | Poll attempt/deadline cap retains an explicit unresolved/last-observation result. | SN, OA, SH, SF; OA metadata limitations |
| M08 | Remote cancel acknowledgement and eventual terminal outcome remain distinct. | SN `CancelObservation`; local stop separate |
| M09 | Partial/staged output is committed only after validated completion. | SF app shards; SH bulk sink; MG/GD application checkpoint obligation |
| M10 | Private outputs, renamed dependencies and downstream extension preserve public usability. | `renamed`, `compat`, core private-output fixtures; maintenance extensions |
| M11 | SDK dependency/features work independently without changing sibling transport behavior. | SN gzip incident/local fix; isolated SDK manifests and historical gate records; no fresh verification here |

Count: 88 scenarios (R: request/response, E: errors, P: pagination, C: credentials, X: replay/retry/admission, S: streaming, U: uploads, M: maintenance/representation).

Results per SDK are recorded in `COVERAGE.md` (raw counts per category, failures and unattempted rows visible).
