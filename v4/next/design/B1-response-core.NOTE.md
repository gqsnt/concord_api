Glue removed (hand, net nonblank): notion 15 = holder 7 + Debug 7 + decode/pack 2→1; import/re-export net 0.
Glue removed: slack 0; warning, response_metadata, custom HTTP meta and envelope routing stay intact.
Glue removed: nager-holidays 0; HolidayResponse and its SDK error policy stay intact.
New concepts: core Response<T> and Received::decode_response(); optional core KeepHead unused; no DSL addition.
C1: All declaration words remain provider facts; all three declarations are unchanged.
C2: Shared result plumbing moves to core; provider validation, error parsing and traversal stay in Rust.
C3: Nager's declaration, result shape, policy and calling syntax are unchanged.
C4: Adoption is an explicit re-export and helper call; no inferred envelope projection.
C5: No new macro syntax or diagnostics; ordinary Rust checks the re-export and helper result types.
C6: Response is the value plus the head it came with.
C7: Caller transports, prepare(), receive(), into_parts() and send_raw() keep their existing access.
C8: The helper preserves the existing decode path; no extra requests, retries, actors or collection.
C9: Existing wire/canary suites retained; new design acceptance test covers non-Debug and non-formatting payloads; none run.
Simple control changed? No production change; only an offline caller example was added.
Does the declaration read better than before? No: it is identical; the benefit is 15 fewer lines of Notion Rust glue.
