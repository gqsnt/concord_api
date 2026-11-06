# Integration record — Open-Meteo SDK

## Between waves 4-5 (CSV input ending inside a quoted field)

Coordinator migration after Concord's CSV fix (R4, merged `aa73712a`). Not an independent
review.

**Verdict: adopted (guard removed).** The author's FRICTION.md F4 recorded that the frozen
library accepted a body ending inside a quoted field as complete, and `src/csv.rs` refused an
odd number of quote bytes before parsing. Concord now refuses that case itself (terminal `Format`
error "the CSV input ends inside a quoted field" on both the finite and incremental paths), so the
guard is removed: 11 lines. The guard was also wrong in general: it refused valid input with a
stray quote inside an unquoted field (`ab"c`) or inside a comment line.

Test change (`tests/protocol.rs`): `frozen_csv_decoder_quote_eof_reproduction_and_sdk_guard`
pinned the frozen behaviour (6 records, `EndOfInput`); renamed
`csv_ending_inside_a_quoted_field_is_refused`, it now asserts 5 records, a terminal error and
`Completion::Failed` from the raw reader, and the SDK's `ResponseErrorKind::Csv` carrying the
library's label. Caller-visible change: the error label for this case.
