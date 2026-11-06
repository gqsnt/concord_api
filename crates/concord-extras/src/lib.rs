//! The standard formats of Concord, written against the public contracts of
//! [`concord_core`]: [`Encode`](concord_core::Encode) and [`Decode`](concord_core::Decode).
//!
//! | Format | Feature | Body (`Encode<T>`) | Output (`Decode<T>`): response head check | `Accept` suggestion |
//! |---|---|---|---|---|
//! | `Json` | `json` | any `T: Serialize`, `application/json` | any `T: DeserializeOwned`; exactly one `Content-Type`, `application/json` or `application/*+json`, charset absent or once and UTF-8 or US-ASCII | `application/json` |
//! | [`Text`] | — | `String`, `text/plain; charset=utf-8` | `String` (UTF-8); any `Content-Type`, also a missing, repeated or malformed one; refused only when a single well-formed one declares a charset other than UTF-8 or US-ASCII (also among repeated charset parameters) | none |
//! | [`Binary`] | — | `Bytes`, `Vec<u8>`, `application/octet-stream` | `Bytes`, `Vec<u8>`; nothing checked (`image/png` is fine) | none |
//! | `Form` | `form` | any `T: Serialize` (flat), `application/x-www-form-urlencoded` | any `T: DeserializeOwned`; exactly one `application/x-www-form-urlencoded`, charset absent or once and UTF-8 or US-ASCII | `application/x-www-form-urlencoded` |
//! | `jsonl::JsonLines` | `jsonl` | `Vec<T: Serialize>` or a lazy `jsonl::StreamRecords<S>`, compact JSON with an LF after every record, `application/jsonl` | `Vec<T: DeserializeOwned>`; exactly one `application/jsonl`, `application/jsonlines` or `application/x-jsonlines`, charset absent or once and UTF-8 or US-ASCII; blank lines and a byte order mark refused | `application/jsonl` |
//! | `jsonl::Ndjson` | `jsonl` | as `JsonLines`, `application/x-ndjson` | as `JsonLines` with `application/x-ndjson` or `application/ndjson`; blank lines ignored | `application/x-ndjson` |
//! | [`IgnoreContentType<F>`] | — | `F`'s | `F`'s, without its `Content-Type` check | `F`'s |
//! | `multipart::Multipart` | `multipart` | `multipart::MultipartBody`: `multipart/form-data` or `multipart/related` parts held in memory, `multipart/<subtype>; boundary=...` chosen per body so that it occurs in no part (a runtime media type), sent with `Content-Length` | — | none |
//! | `csv::Csv<H, D, P>` | `csv` | `Vec<T: Serialize>`, `csv::Rows`, a lazy `csv::StreamRows`, `csv::RawTable`; `text/csv; header=present\|absent` from `H` | `Vec<T: DeserializeOwned>`, `csv::RawTable`; exactly one `text/csv` (profile can relax), charset absent or UTF-8/US-ASCII, a `header=` parameter agreeing with `H`; also an incremental reader | `text/csv` |
//! | `sse::Sse` | `sse` | — | none (incremental only: `-> stream sse::Event` returns `sse::SseEvents`); exactly one `text/event-stream`, charset absent or once and UTF-8 or US-ASCII | `text/event-stream` |
//!
//! Head checks run before the body is read; a refusal is an
//! [`ErrorKind::Metadata`](concord_core::ErrorKind::Metadata) error that keeps the response
//! unread. The `jsonl` module also reads JSON Lines and consecutive JSON values (a Kubernetes
//! watch) incrementally as a response arrives, on the `stream` infrastructure; the `sse` module
//! reads server-sent events the same way. None of these formats is known to the `api!` macro: a
//! statement names them by their type path (`format Json;`), exactly like a format of your own.
//! Applications normally get them through the `concord` crate.

mod binary;
#[cfg(feature = "csv")]
pub mod csv;
mod encoded;
#[cfg(feature = "form")]
mod form;
mod ignore;
#[cfg(feature = "json")]
mod json;
#[cfg(feature = "jsonl")]
pub mod jsonl;
mod media;
#[cfg(feature = "multipart")]
pub mod multipart;
#[cfg(feature = "sse")]
pub mod sse;
#[cfg(feature = "stream")]
pub mod stream;
mod text;

pub use binary::Binary;
pub use encoded::Encoded;
#[cfg(feature = "form")]
pub use form::Form;
pub use ignore::IgnoreContentType;
#[cfg(feature = "json")]
pub use json::Json;
pub use text::Text;
