//! Whole-body readers and bounded record readers, each implementing `concord_core::Reader`.
//!
//! [`Sse`] reads server-sent events. With the `json` feature, `JsonLines` reads JSON
//! values separated by lines. Both return a [`RecordStream`] after the response head.
//! Errors during the body are stream items. Their type examples show complete
//! descriptions and calls.

mod records;
#[cfg(feature = "json")]
#[cfg_attr(docsrs, doc(cfg(feature = "json")))]
pub use records::JsonLine;
#[cfg(feature = "json")]
#[cfg_attr(docsrs, doc(cfg(feature = "json")))]
pub use records::JsonLines;
pub use records::{
    EndOfRecords, RecordError, RecordFail, RecordStream, Sse, SseEvent, Step, TryMap,
};

use concord_core::read::{Incoming, ReadError, Reader};
use concord_core::{ByteStream, MIB};

#[cfg(feature = "json")]
#[path = "json_decode.rs"]
mod json_decode;

/// What a reader of a whole body takes at most, unless it is told another limit.
// PANIC-SAFETY: The default 10 MiB limit fits usize on supported 32/64-bit targets.
const LIMIT: usize = 10 * MIB;

/// Reads JSON into any type that `serde` can deserialize: `read Json;`. At most 10 MiB.
///
/// Skips a UTF-8 byte-order mark. Decode failures identify fields and indexes
/// where Serde provides them; buffered deserialization through flattened fields or
/// tagged enums may lose fields, indexes, or line and column information.
/// Keys taken from the data are reported as `[key]`. A failed decode also reports
/// when the response's content type was not JSON.
#[cfg(feature = "json")]
#[cfg_attr(docsrs, doc(cfg(feature = "json")))]
#[derive(Clone, Copy, Default)]
pub struct Json {
    limit: Option<usize>,
    strict: bool,
}

#[cfg(feature = "json")]
impl std::fmt::Debug for Json {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Json").finish_non_exhaustive()
    }
}

/// The `Json` reader as a value, for a description to name: `read Json.limit(50 * concord::MIB);`.
#[cfg(feature = "json")]
#[cfg_attr(docsrs, doc(cfg(feature = "json")))]
// PANIC-SAFETY: The reader value intentionally shares its type name in the DSL; no panic lint is relaxed.
#[allow(non_upper_case_globals)]
pub const Json: Json = Json {
    limit: None,
    strict: false,
};

#[cfg(feature = "json")]
impl Json {
    /// Sets a limit other than 10 MiB.
    pub fn limit(mut self, bytes: usize) -> Json {
        self.limit = Some(bytes);
        self
    }

    /// Rejects a response whose content type is not JSON, before reading it.
    pub fn strict(mut self) -> Json {
        self.strict = true;
        self
    }

    /// Decodes JSON that is already in memory, with the error that `read Json;` gives for a
    /// body that does not fit the type.
    ///
    /// It is for an arm that decodes the body itself, as for an API that reports its errors
    /// inside a `200`. [`Fail::from_read`](concord_core::Fail::from_read) makes the error the
    /// failure of the arm, and the call's error then names the location of the fault, as it
    /// does under `read Json;` (`name could not be decoded`, below), with the decoder's
    /// message, its line and its column in `details()`, and at most 64 KiB of its input in
    /// `body()`, which `details()` names `decoder input`. A byte order mark at the start is
    /// skipped.
    ///
    /// Three things that the reader does are not done here, because the bytes are already in
    /// memory. No byte limit applies: the limit is the one of the reader that read the bytes
    /// (10 MiB for `read Binary;`; `read Binary.limit(n);` sets another). `strict` does not
    /// apply. And the content type is not looked at, so the error never adds
    /// `(unexpected content type)`.
    ///
    /// The value of the API below sits beside `ok`, not under a key of its own, so the
    /// client reads the bytes (`read Binary;`) and its arm decodes them twice: once for
    /// `ok`, once for the value or the problem. When the first pass succeeds with `ok: true`,
    /// the second pass meets a fault exactly where `read Json;` would. The first pass can fail
    /// where `read Json;` would not: it can catch broken JSON, where its syntax breaks (which
    /// can be after a value that does not fit), though not every fault of a part that it skips
    /// (bytes that are not UTF-8 inside a string pass it); and it fails on a body with `ok`
    /// twice.
    ///
    /// ```
    /// use concord::{bytes::Bytes, error::Kind, fake, reply, Binary, Fail, Json};
    /// use serde::{de::DeserializeOwned, Deserialize};
    ///
    /// concord::api! {
    ///     pub client Chat {
    ///         base "https://chat.example.test/api";
    ///         read Binary;
    ///         problem ChatProblem read Json;
    ///         status { 2xx { body } => envelope(body)? }
    ///
    ///         GET info() -> Channel { path "conversations.info"; }
    ///     }
    /// }
    ///
    /// /// `{"ok": false, "error": "channel_not_found"}`, with the status `200`.
    /// #[derive(Debug, Deserialize)]
    /// pub struct ChatProblem {
    ///     pub error: String,
    /// }
    ///
    /// /// `{"ok": true, "id": "C1", "name": "general"}`.
    /// #[derive(Debug, Deserialize)]
    /// pub struct Channel {
    ///     pub id: String,
    ///     pub name: String,
    /// }
    ///
    /// /// The arm of every 2xx: `ok` says whether the body is the value or the problem.
    /// pub fn envelope<T: DeserializeOwned>(body: Bytes) -> Result<T, Fail<ChatProblem>> {
    ///     #[derive(Deserialize)]
    ///     struct Head {
    ///         ok: bool,
    ///     }
    ///     let head: Head = Json::decode(&body).map_err(Fail::from_read)?;
    ///     if head.ok {
    ///         Json::decode(&body).map_err(Fail::from_read)
    ///     } else {
    ///         let problem: ChatProblem = Json::decode(&body).map_err(Fail::from_read)?;
    ///         Err(problem)?
    ///     }
    /// }
    ///
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// let chat = Chat::new().with_transport(fake(|_| {
    ///     Ok(reply(200, r#"{"ok": true, "id": "C1", "name": 5}"#))
    /// }));
    /// let error = chat.info().await.unwrap_err();
    /// assert_eq!(error.kind(), Kind::Mismatch);
    /// assert_eq!(
    ///     error.to_string(),
    ///     "mismatch: status 200, name could not be decoded, in chat.info"
    /// );
    /// assert!(error.details().to_string().contains("at line 1 column 34"));
    ///
    /// let chat = Chat::new().with_transport(fake(|_| {
    ///     Ok(reply(200, r#"{"ok": false, "error": "channel_not_found"}"#))
    /// }));
    /// let error = chat.info().await.unwrap_err();
    /// assert_eq!(error.kind(), Kind::Refused);
    /// assert_eq!(error.problem().map(|p| p.error.as_str()), Some("channel_not_found"));
    /// # }
    /// ```
    pub fn decode<T: serde::de::DeserializeOwned>(json: impl AsRef<[u8]>) -> Result<T, ReadError> {
        let json = json.as_ref();
        json_decode::decode(json, Some("application/json")).map_err(|error| {
            // An error keeps at most 64 KiB; one byte more tells it that the bytes were cut.
            // PANIC-SAFETY: The constant byte count fits usize on supported 32/64-bit targets.
            let kept = json.len().min(64 * concord_core::KIB + 1);
            not_json(
                concord_core::bytes::Bytes::copy_from_slice(json.get(..kept).unwrap_or_default()),
                error,
            )
        })
    }
}

#[cfg(feature = "json")]
impl<T: serde::de::DeserializeOwned + Send> Reader<T> for Json {
    async fn read(&self, body: Incoming) -> Result<T, ReadError> {
        let label = body.content_type().map(str::to_owned);
        if self.strict && !json_decode::is_json(label.as_deref()) {
            return Err(ReadError::wrong_content_type());
        }
        let bytes = body.bytes(self.limit.unwrap_or(LIMIT)).await?;
        match json_decode::decode(&bytes, label.as_deref()) {
            Ok(value) => Ok(value),
            Err(error) => Err(not_json(bytes, error)),
        }
    }
}

#[cfg(feature = "json")]
fn not_json(bytes: concord_core::bytes::Bytes, error: json_decode::Failure) -> ReadError {
    let _ = &error.source; // Decoder diagnostics are never passed as a cause.
    let failure = ReadError::not_the_type(bytes)
        .at(error.place)
        .with_details(error.details);
    if error.unexpected_label {
        failure.label_was_not_expected()
    } else {
        failure
    }
}

/// Reads the whole body as UTF-8 text: `read Text;`. At most 10 MiB.
#[derive(Clone, Copy, Default)]
pub struct Text {
    limit: Option<usize>,
}

impl std::fmt::Debug for Text {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Text").finish_non_exhaustive()
    }
}

/// The `Text` reader as a value, for a description to name: `read Text.limit(concord::MIB);`.
// PANIC-SAFETY: The reader value intentionally shares its type name in the DSL; no panic lint is relaxed.
#[allow(non_upper_case_globals)]
pub const Text: Text = Text { limit: None };

impl Text {
    /// Sets a limit other than 10 MiB.
    pub fn limit(mut self, bytes: usize) -> Text {
        self.limit = Some(bytes);
        self
    }
}

impl Reader<String> for Text {
    async fn read(&self, body: Incoming) -> Result<String, ReadError> {
        let bytes = body.bytes(self.limit.unwrap_or(LIMIT)).await?;
        match std::str::from_utf8(&bytes) {
            Ok(text) => Ok(text.to_owned()),
            Err(error) => Err(ReadError::not_the_type(bytes).with_details(error.to_string())),
        }
    }
}

/// Reads the whole body as it is: `read Binary;`, for `-> bytes::Bytes`. At most 10 MiB.
#[derive(Clone, Copy, Default)]
pub struct Binary {
    limit: Option<usize>,
}

impl std::fmt::Debug for Binary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Binary").finish_non_exhaustive()
    }
}

/// The `Binary` reader as a value, for a description to name: `read Binary.limit(100 * concord::MIB);`.
// PANIC-SAFETY: The reader value intentionally shares its type name in the DSL; no panic lint is relaxed.
#[allow(non_upper_case_globals)]
pub const Binary: Binary = Binary { limit: None };

impl Binary {
    /// Sets a limit other than 10 MiB.
    pub fn limit(mut self, bytes: usize) -> Binary {
        self.limit = Some(bytes);
        self
    }
}

impl Reader<concord_core::bytes::Bytes> for Binary {
    async fn read(&self, body: Incoming) -> Result<concord_core::bytes::Bytes, ReadError> {
        body.bytes(self.limit.unwrap_or(LIMIT)).await
    }
}

/// Reads nothing: hands the body over as a [`ByteStream`], with no limit: `read Stream;`.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stream;

impl Reader<ByteStream> for Stream {
    async fn read(&self, body: Incoming) -> Result<ByteStream, ReadError> {
        Ok(body.stream())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "json")]
    #[tokio::test]
    async fn c5_3_json_arm_failures_keep_the_refusal_with_each_reader_setting() {
        use concord_core::error::{Kept, Kind};
        use concord_core::transport::TryInfo;
        for reader in [Json, Json.strict(), Json.limit(1)] {
            let error: concord_core::Error<String> =
                concord_core::__private::read_body::<u32, _, _>(
                    &reader,
                    concord_core::reply_with(
                        429,
                        &[("content-type", "text/html"), ("x-id", "one")],
                        "bad body",
                    ),
                    &TryInfo::new("client.get", 1),
                )
                .await
                .unwrap_err();
            assert_eq!(error.kind(), Kind::Refused);
            assert!(error.is_status(429));
            assert_eq!(error.headers().unwrap().get("x-id"), Some("one"));
            assert_eq!(error.body(), Some(b"bad body".as_slice()));
            assert_eq!(error.kept(), Some(Kept::Complete));
            assert!(error.problem().is_none());
        }
    }

    #[cfg(feature = "json")]
    #[tokio::test]
    async fn c5_2_empty_json_omits_a_label_sentence_when_no_label_arrived() {
        use concord_core::transport::TryInfo;
        let error: concord_core::Error = concord_core::__private::read_body::<u32, _, _>(
            &Json,
            concord_core::reply(204, ""),
            &TryInfo::new("client.get", 1),
        )
        .await
        .unwrap_err();
        assert_eq!(
            error.to_string(),
            "mismatch: status 204, the body is empty, in client.get"
        );
    }

    #[cfg(feature = "json")]
    #[test]
    fn json_debug_hides_reader_settings() {
        let reader = Json.limit(1234567).strict();
        assert_eq!(format!("{reader:?}"), "Json { .. }");
        assert_eq!(format!("{reader:#?}"), "Json { .. }");
    }

    #[test]
    fn text_debug_hides_reader_settings() {
        let reader = Text.limit(1234567);
        assert_eq!(format!("{reader:?}"), "Text { .. }");
        assert_eq!(format!("{reader:#?}"), "Text { .. }");
    }

    #[test]
    fn binary_debug_hides_reader_settings() {
        let reader = Binary.limit(1234567);
        assert_eq!(format!("{reader:?}"), "Binary { .. }");
        assert_eq!(format!("{reader:#?}"), "Binary { .. }");
    }
}
