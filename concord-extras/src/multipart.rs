//! Multipart/form-data encoding (RFC 7578), independent of any transport.
use concord_core::{body::BodyStream, bytes::Bytes, Body};
use futures_util::StreamExt;
use std::{fmt, io};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Starts a multipart/form-data body. Add text and files, then call [`Multipart::finish`].
///
/// In a description: `body multipart().text("caption", caption).file("photo", name,
/// "image/jpeg", jpeg.clone()).finish();`. Each part is sent as a line `--boundary`, its
/// header lines (`Content-Disposition: form-data; name="caption"`, and for a file
/// `Content-Disposition: form-data; name="photo"; filename="mast.jpg"` then
/// `Content-Type: image/jpeg`, spelled exactly so), an empty line, and its bytes; a line
/// `--boundary--` ends the body, and every line ends with `\r\n`. The documentation of
/// `api!` shows the whole body of one request, checked by a test.
pub fn multipart() -> Multipart {
    Multipart::default()
}

/// An ordered collection of multipart fields. Debug shows only the part count.
#[derive(Default)]
pub struct Multipart {
    parts: Vec<Part>,
    error: Option<MultipartError>,
}
struct Part {
    header: String,
    body: Body,
}

/// The reason a multipart body could not be built.
///
/// Match failures with a fallback arm, so that future causes remain compatible.
///
/// ```compile_fail
/// # mod concord { pub use concord_extras::MultipartError; }
/// fn exhaustive(error: concord::MultipartError) {
///     match error {
///         concord::MultipartError::Name => (),
///         concord::MultipartError::ContentType(_) => (),
///         concord::MultipartError::Randomness => (),
///     }
/// }
/// ```
#[derive(Debug)]
#[non_exhaustive]
pub enum MultipartError {
    /// A field name or file name contains a control character.
    Name,
    /// A part's content type is not a legal HTTP header value.
    ContentType(concord_core::http::header::InvalidHeaderValue),
    /// The operating system could not supply a streaming boundary.
    Randomness,
}
impl fmt::Display for MultipartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Name => "invalid multipart field or file name",
            Self::ContentType(_) => "invalid multipart content type",
            Self::Randomness => "multipart boundary randomness unavailable",
        })
    }
}
impl std::error::Error for MultipartError {}
impl fmt::Debug for Multipart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Multipart")
            .field("parts", &self.parts.len())
            .finish()
    }
}
impl Multipart {
    /// Adds a UTF-8 text field at this position. Control characters in its name
    /// are reported by `finish()`. Quotes are written as `%22`; other text is kept as it is.
    /// A quote and the literal text `%22` therefore encode the same way; names need not be unique.
    ///
    /// ```compile_fail
    /// #![deny(unused_must_use)]
    /// # mod concord { pub use concord_extras::multipart; }
    /// concord::multipart().text("a", "b");
    /// ```
    #[must_use]
    pub fn text(mut self, name: &str, value: impl ToString) -> Self {
        self.check_name(name);
        self.parts.push(Part {
            header: format!(
                "Content-Disposition: form-data; name=\"{}\"\r\n\r\n",
                quoted(name)
            ),
            body: value.to_string().into(),
        });
        self
    }

    /// Adds a file, preserving order. Control characters in either name and an invalid content
    /// type are reported by `finish()`. Quotes are written as `%22`; other text is kept as it is.
    /// A quote and the literal text `%22` therefore encode the same way; names need not be unique.
    ///
    /// ```compile_fail
    /// #![deny(unused_must_use)]
    /// # mod concord { pub use concord_extras::multipart; }
    /// concord::multipart().file("f", "x", "text/plain", "body");
    /// ```
    #[must_use]
    pub fn file(
        mut self,
        name: &str,
        file_name: &str,
        content_type: &str,
        body: impl Into<Body>,
    ) -> Self {
        self.check_name(name);
        self.check_name(file_name);
        if let Err(error) = concord_core::http::HeaderValue::from_str(content_type) {
            self.error.get_or_insert(MultipartError::ContentType(error));
        }
        self.parts.push(Part {
            header: format!("Content-Disposition: form-data; name=\"{}\"; filename=\"{}\"\r\nContent-Type: {}\r\n\r\n",
                quoted(name), quoted(file_name), content_type),
            body: body.into(),
        });
        self
    }

    fn check_name(&mut self, name: &str) {
        if name.chars().any(char::is_control) {
            self.error.get_or_insert(MultipartError::Name);
        }
    }

    /// Produces bytes if every part is in memory, otherwise a one-shot stream.
    /// Buffered inputs use the first boundary that does not occur in them, from the fixed sequence
    /// `concord-00000000000000000000000000000000`, `concord-00000000000000000000000000000001`, ….
    /// Streaming inputs use 128 random bits. A boundary inside a stream is an error,
    /// detected across chunks before the colliding bytes are yielded.
    /// All underlying streams are claimed together when the result is taken.
    ///
    /// # Errors
    /// A control character in a field or file name, an invalid content type, or
    /// unavailable system randomness. No source is claimed.
    pub fn finish(self) -> Result<Body, MultipartError> {
        if self.parts.iter().all(|part| part.body.as_bytes().is_some()) {
            let mut index = 0u128;
            self.finish_with(|| {
                let boundary = format!("concord-{index:032x}");
                // PANIC-SAFETY: Each colliding candidate needs a distinct stored substring; at most isize::MAX can exist, below u128::MAX.
                index += 1;
                Ok(boundary)
            })
        } else {
            self.finish_with(random_boundary)
        }
    }

    fn finish_with(
        self,
        mut candidate: impl FnMut() -> Result<String, MultipartError>,
    ) -> Result<Body, MultipartError> {
        if let Some(error) = self.error {
            return Err(error);
        }
        let boundary = loop {
            let value = candidate()?;
            if self.parts.iter().all(|part| {
                !contains(part.header.as_bytes(), value.as_bytes())
                    && part
                        .body
                        .as_bytes()
                        .is_none_or(|bytes| !contains(bytes, value.as_bytes()))
            }) {
                break value;
            }
        };
        let mut parts = Vec::new();
        for part in self.parts {
            parts.push(Body::from(format!("--{boundary}\r\n{}", part.header)));
            parts.push(if part.body.as_bytes().is_some() {
                part.body
            } else {
                let boundary = boundary.as_bytes().to_vec();
                part.body
                    .map_stream(move |stream| checked(stream, boundary.clone()))
            });
            parts.push(Body::from("\r\n"));
        }
        parts.push(Body::from(format!("--{boundary}--\r\n")));
        Ok(Body::concat(parts).content_type(format!("multipart/form-data; boundary={boundary}")))
    }
}
fn random_boundary() -> Result<String, MultipartError> {
    random_boundary_with(getrandom::fill)
}
fn random_boundary_with(
    fill: impl FnOnce(&mut [u8]) -> Result<(), getrandom::Error>,
) -> Result<String, MultipartError> {
    let mut random = [0; 16];
    fill(&mut random).map_err(|_| MultipartError::Randomness)?;
    let hex: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
    Ok(format!("concord-{hex}"))
}
fn quoted(text: &str) -> String {
    text.replace('"', "%22")
}
fn contains(bytes: &[u8], boundary: &[u8]) -> bool {
    // PANIC-SAFETY: Both generated boundary forms have a nonzero fixed length of 40 bytes.
    bytes.windows(boundary.len()).any(|b| b == boundary)
}
fn checked(stream: BodyStream, boundary: Vec<u8>) -> BodyStream {
    Box::pin(futures_util::stream::try_unfold(
        (stream, Vec::new(), boundary, false),
        |(mut stream, mut kept, boundary, mut ended)| async move {
            loop {
                if ended {
                    return Ok::<_, BoxError>((!kept.is_empty()).then(|| {
                        (
                            Bytes::from(std::mem::take(&mut kept)),
                            (stream, kept, boundary, true),
                        )
                    }));
                }
                match stream.next().await {
                    Some(chunk) => {
                        kept.extend_from_slice(&chunk?);
                        if contains(&kept, &boundary) {
                            return Err(io::Error::other(
                                "multipart boundary occurred in a stream part",
                            )
                            .into());
                        }
                        // PANIC-SAFETY: Both generated boundary forms have a nonzero fixed length of 40 bytes.
                        let count = kept.len().saturating_sub(boundary.len() - 1);
                        if count > 0 {
                            // PANIC-SAFETY: count is computed by saturating subtraction from kept.len(), so it cannot exceed that length.
                            let rest = kept.split_off(count);
                            return Ok(Some((Bytes::from(kept), (stream, rest, boundary, false))));
                        }
                    }
                    None => ended = true,
                }
            }
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixed() -> Result<String, MultipartError> {
        Ok("test-boundary".into())
    }
    async fn bytes(body: Body) -> Result<Vec<u8>, BoxError> {
        let mut stream = body.into_stream()?;
        let mut bytes = Vec::new();
        while let Some(chunk) = stream.next().await {
            bytes.extend_from_slice(&chunk?);
        }
        Ok(bytes)
    }
    #[test]
    fn c2_16_rfc_7578_bytes_and_quoted_names() {
        let body = multipart()
            .text("chat\"id", "7")
            .file("photo", "a\".jpg", "image/jpeg", vec![0, 1, 255])
            .finish_with(fixed)
            .unwrap();
        assert_eq!(
            body.__content_type(),
            Some("multipart/form-data; boundary=test-boundary")
        );
        let mut expected = b"--test-boundary\r\nContent-Disposition: form-data; name=\"chat%22id\"\r\n\r\n7\r\n--test-boundary\r\nContent-Disposition: form-data; name=\"photo\"; filename=\"a%22.jpg\"\r\nContent-Type: image/jpeg\r\n\r\n".to_vec();
        expected.extend_from_slice(&[0, 1, 255]);
        expected.extend_from_slice(b"\r\n--test-boundary--\r\n");
        assert_eq!(body.as_bytes(), Some(expected.as_slice()));
    }
    #[test]
    fn boundary_cannot_occur_in_memory_parts_or_headers() {
        let mut candidates = ["in-name", "in-value", "in-file", "safe"].into_iter();
        let body = multipart()
            .text("in-name", "in-value")
            .file("data", "a", "text/plain", "in-file")
            .finish_with(|| Ok(candidates.next().unwrap().into()))
            .unwrap();
        assert_eq!(
            body.__content_type(),
            Some("multipart/form-data; boundary=safe")
        );
        let body: Body = multipart()
            .text("a", 1)
            .file("b", "c", "text/plain", "d")
            .finish()
            .unwrap();
        let boundary = body
            .__content_type()
            .unwrap()
            .split_once("boundary=")
            .unwrap()
            .1;
        assert!(boundary.len() <= 70);
        assert!(!["a", "1", "b", "c", "d"]
            .iter()
            .any(|s| s.contains(boundary)));
    }
    #[tokio::test]
    async fn a_stream_part_produces_the_same_wire_bytes() {
        let stream = futures_util::stream::iter([
            Ok::<_, io::Error>(Bytes::from_static(b"ab")),
            Ok(Bytes::from_static(b"cd")),
        ]);
        let body = multipart()
            .text("name", "value")
            .file("file", "a", "text/plain", Body::stream(stream))
            .finish_with(fixed)
            .unwrap();
        assert!(body.as_bytes().is_none());
        let expected = multipart()
            .text("name", "value")
            .file("file", "a", "text/plain", "abcd")
            .finish_with(fixed)
            .unwrap();
        assert_eq!(bytes(body).await.unwrap(), expected.as_bytes().unwrap());
    }
    #[tokio::test]
    async fn stream_collisions_are_rejected_across_chunks() {
        let stream = futures_util::stream::iter([
            Ok::<_, io::Error>(Bytes::from_static(b"test-")),
            Ok(Bytes::from_static(b"boundary")),
        ]);
        let body = multipart()
            .file("file", "a", "text/plain", Body::stream(stream))
            .finish_with(fixed)
            .unwrap();
        assert!(bytes(body)
            .await
            .unwrap_err()
            .to_string()
            .contains("boundary"));
    }
    #[tokio::test]
    async fn source_errors_and_one_shot_claims_are_preserved() {
        let source = Body::stream(futures_util::stream::iter([Err::<Bytes, _>(
            io::Error::other("broken source"),
        )]));
        let copy = source.clone();
        let body = multipart()
            .file("file", "a", "text/plain", source)
            .finish_with(fixed)
            .unwrap();
        assert!(copy.into_stream().is_ok());
        assert!(bytes(body)
            .await
            .unwrap_err()
            .to_string()
            .contains("already used"));
        let body = multipart()
            .file(
                "a",
                "b",
                "text/plain",
                Body::stream(futures_util::stream::empty::<Result<Bytes, io::Error>>()),
            )
            .finish()
            .unwrap();
        let copy = body.clone();
        bytes(body).await.unwrap();
        assert!(copy.into_stream().is_err());
        let body = multipart()
            .file(
                "a",
                "b",
                "text/plain",
                Body::stream(futures_util::stream::iter([Err::<Bytes, _>(
                    io::Error::other("broken source"),
                )])),
            )
            .finish()
            .unwrap();
        assert!(bytes(body)
            .await
            .unwrap_err()
            .to_string()
            .contains("broken source"));
    }

    #[tokio::test]
    async fn multipart_map_of_memory_is_one_shot_through_clones() {
        // A multipart body with a stream can be sent only once.
        let body = multipart()
            .file("a", "b", "text/plain", Body::from("data").map_stream(|s| s))
            .finish()
            .unwrap();
        assert!(body.as_bytes().is_none());
        let copy = body.clone();
        assert!(contains(&bytes(body).await.unwrap(), b"\r\n\r\ndata\r\n"));
        assert!(copy.into_stream().is_err());
    }

    #[test]
    fn buffered_multipart_builds_are_byte_identical() {
        let build = || {
            multipart()
                .text("chat_id", 7)
                .file("photo", "a.jpg", "image/jpeg", "same photo")
                .finish()
                .unwrap()
        };
        let a = build();
        let b = build();
        assert_eq!(a.as_bytes(), b.as_bytes());
        assert_eq!(a.__content_type(), b.__content_type());
    }

    #[test]
    fn buffered_boundary_skips_the_first_fixed_candidates() {
        let a = "concord-00000000000000000000000000000000";
        let b = "concord-00000000000000000000000000000001";
        let c = "concord-00000000000000000000000000000002";
        let body = multipart().text(a, format!("{b} {c}")).finish().unwrap();
        assert_eq!(
            body.__content_type(),
            Some("multipart/form-data; boundary=concord-00000000000000000000000000000003")
        );
    }

    #[test]
    fn invalid_content_type_is_a_finish_error_without_claiming_a_stream() {
        let source = Body::stream(futures_util::stream::empty::<Result<Bytes, io::Error>>());
        let builder =
            multipart().file("photo", "a.jpg", "image/jpeg\r\nx-bad: yes", source.clone());
        assert!(matches!(
            builder.finish(),
            Err(MultipartError::ContentType(_))
        ));
        assert!(source.into_stream().is_ok());
    }

    #[test]
    fn c2_16_every_control_is_rejected_in_field_and_file_names() {
        for control in (0u8..=31).chain(std::iter::once(127)) {
            let name = format!("before{}after", char::from(control));
            assert!(
                matches!(
                    multipart().text(&name, "value").finish(),
                    Err(MultipartError::Name)
                ),
                "text field {control}"
            );
            for filename in [false, true] {
                let source =
                    Body::stream(futures_util::stream::empty::<Result<Bytes, io::Error>>());
                let builder = if filename {
                    multipart().file("field", &name, "text/plain", &source)
                } else {
                    multipart().file(&name, "file.txt", "text/plain", &source)
                };
                assert!(
                    matches!(builder.finish(), Err(MultipartError::Name)),
                    "file position {filename}, control {control}"
                );
                assert!(source.into_stream().is_ok());
            }
        }
    }

    #[tokio::test]
    async fn rebuilt_multipart_rejects_a_used_inner_stream_before_any_byte() {
        // The outer body is new, but its input stream has already been consumed.
        let source = Body::stream(futures_util::stream::iter([Ok::<_, io::Error>(
            Bytes::from_static(b"photo"),
        )]));
        let wrap = || {
            multipart()
                .text("chat_id", 7)
                .file("photo", "a.jpg", "image/jpeg", &source)
                .finish()
                .unwrap()
        };
        bytes(wrap()).await.unwrap();
        let rebuilt = wrap();
        assert!(rebuilt.as_bytes().is_none());
        let error = match rebuilt.into_stream() {
            Ok(_) => panic!("reuse must fail before a stream or multipart header is returned"),
            Err(error) => error,
        };
        assert!(error.to_string().contains("already used"));
    }

    #[tokio::test]
    async fn inspecting_multipart_does_not_touch_any_source() {
        let source = Body::stream(futures_util::stream::iter([Ok::<_, io::Error>(
            Bytes::from_static(b"photo"),
        )]));
        let body = multipart()
            .file("photo", "a.jpg", "image/jpeg", &source)
            .finish()
            .unwrap();
        assert!(body.as_bytes().is_none());
        assert_eq!(bytes(source).await.unwrap(), b"photo");
        assert!(body.into_stream().is_err());
    }

    #[test]
    fn streaming_boundaries_are_distinct_32_digit_hex_strings() {
        let build = || {
            multipart()
                .file(
                    "a",
                    "b",
                    "text/plain",
                    Body::stream(futures_util::stream::empty::<Result<Bytes, io::Error>>()),
                )
                .finish()
                .unwrap()
        };
        let a = build();
        let b = build();
        for body in [&a, &b] {
            let boundary = body
                .__content_type()
                .unwrap()
                .split_once("boundary=")
                .unwrap()
                .1;
            let hex = boundary.strip_prefix("concord-").unwrap();
            assert_eq!(hex.len(), 32);
            assert!(hex.bytes().all(|byte| byte.is_ascii_hexdigit()));
        }
        assert_ne!(a.__content_type(), b.__content_type());
    }

    #[test]
    fn streaming_boundary_encodes_all_sixteen_supplied_bytes() {
        let boundary = random_boundary_with(|bytes| {
            assert_eq!(bytes.len(), 16);
            bytes.copy_from_slice(&[
                0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54,
                0x32, 0x10,
            ]);
            Ok(())
        })
        .unwrap();
        assert_eq!(boundary, "concord-0123456789abcdeffedcba9876543210");
    }

    #[test]
    fn independently_vary_every_input_bit_and_preserve_its_position() {
        let initial = [
            0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54,
            0x32, 0x10,
        ];
        for index in 0..16 {
            for bit in 0..8 {
                let mut supplied = initial;
                supplied[index] ^= 1 << bit;
                let mut fills = 0;
                let result = random_boundary_with(|out| {
                    fills += 1;
                    assert_eq!(out.len(), 16);
                    out.copy_from_slice(&supplied);
                    Ok(())
                })
                .unwrap();
                let expected = format!(
                    "concord-{}",
                    supplied
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>()
                );
                assert_eq!(fills, 1);
                assert_eq!(result, expected, "input byte {index}, bit {bit}");
            }
        }
    }

    #[test]
    fn unavailable_randomness_is_an_error_without_claiming_sources() {
        assert!(matches!(
            random_boundary_with(|_| Err(getrandom::Error::UNSUPPORTED)),
            Err(MultipartError::Randomness)
        ));
        let source = Body::stream(futures_util::stream::empty::<Result<Bytes, io::Error>>());
        let body = multipart()
            .file("a", "b", "text/plain", &source)
            .finish_with(|| Err(MultipartError::Randomness));
        assert!(matches!(body, Err(MultipartError::Randomness)));
        assert!(source.into_stream().is_ok());
    }
}
