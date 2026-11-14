//! Four readers, each a value that implements `concord_core::Reader`.

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
            Err(error) => {
                let _ = &error.source; // Decoder diagnostics are never passed as a cause.
                let failure = ReadError::not_the_type(bytes)
                    .at(error.place)
                    .with_details(error.details);
                Err(if error.unexpected_label {
                    failure.label_was_not_expected()
                } else {
                    failure
                })
            }
        }
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
