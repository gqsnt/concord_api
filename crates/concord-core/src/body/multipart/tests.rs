use std::future::poll_fn;
use std::num::NonZeroUsize;

use super::*;
use crate::body::PreparedBody;

struct InvalidMultipartFactory;

impl MultipartFactory for InvalidMultipartFactory {
    fn create(&self) -> Result<MultipartBody, BodyError> {
        let name = format!(
            "{}factory-name-sentinel",
            "n".repeat(MAX_MULTIPART_NAME_BYTES)
        );
        Ok(MultipartBody {
            metadata_bytes: name.len(),
            parts: vec![(
                name,
                MultipartPart::text("factory-value").expect("bounded factory text"),
            )],
        })
    }
}

fn text(value: &str) -> MultipartPart {
    MultipartPart::text(value).expect("bounded multipart text")
}

async fn serialize(
    body: MultipartBody,
    boundary: &MultipartBoundary,
) -> Result<Vec<u8>, BodyError> {
    let mut stream = body.into_stream(boundary)?;
    let mut output = Vec::new();
    while let Some(chunk) = poll_fn(|context| stream.as_mut().poll_next(context)).await {
        output.extend_from_slice(&chunk?);
    }
    Ok(output)
}

#[test]
fn validation_and_debug_are_structural_and_redacted() {
    let secret = "multipart-value-sentinel";
    let filename = "filename-sentinel.txt";
    let part = MultipartPart::text(secret)
        .expect("bounded text")
        .file_name(filename)
        .expect("valid filename")
        .content_type("text/plain; charset=utf-8")
        .expect("valid media type");
    let part_debug = format!("{part:?}");
    assert!(!part_debug.contains(secret));
    assert!(!part_debug.contains(filename));

    let body = MultipartBody::new()
        .part("field", part)
        .expect("valid part");
    let body_debug = format!("{body:?}");
    assert!(!body_debug.contains(secret));
    assert!(!body_debug.contains(filename));

    assert!(MultipartBody::new().part("bad\r\nname", text("x")).is_err());
    assert!(
        MultipartPart::bytes(Bytes::new())
            .expect("bounded bytes")
            .file_name("bad\nfile")
            .is_err()
    );
    assert!(
        MultipartPart::bytes(Bytes::new())
            .expect("bounded bytes")
            .content_type("*/json")
            .is_err()
    );
}

#[test]
fn exact_part_count_and_first_excess_part_is_not_retained() {
    let mut body = MultipartBody::new();
    for _ in 0..MAX_MULTIPART_PARTS {
        body.push_part("p", text("x")).expect("part-count limit");
    }
    assert_eq!(body.parts.len(), MAX_MULTIPART_PARTS);

    let error = body
        .push_part("first-excess-part-sentinel", text("not-retained"))
        .expect_err("first excess part");
    assert_eq!(error.kind(), BodyErrorKind::LimitExceeded);
    assert_eq!(body.parts.len(), MAX_MULTIPART_PARTS);
    assert!(!format!("{error:?}").contains("first-excess-part-sentinel"));
}

#[test]
fn name_filename_and_content_type_have_exact_borrowed_limits() {
    let exact_name = "n".repeat(MAX_MULTIPART_NAME_BYTES);
    MultipartBody::new()
        .part(exact_name.as_str(), text("x"))
        .expect("exact name limit");
    let excessive_name = format!("{exact_name}name-sentinel");
    let error = MultipartBody::new()
        .part(excessive_name.as_str(), text("x"))
        .expect_err("name first excess");
    assert_eq!(error.kind(), BodyErrorKind::LimitExceeded);
    assert!(!format!("{error:?}").contains("name-sentinel"));

    let exact_filename = "f".repeat(MAX_MULTIPART_FILENAME_BYTES);
    text("x")
        .file_name(exact_filename.as_str())
        .expect("exact filename limit");
    let excessive_filename = format!("{exact_filename}filename-sentinel");
    let error = text("x")
        .file_name(excessive_filename.as_str())
        .expect_err("filename first excess");
    assert_eq!(error.kind(), BodyErrorKind::LimitExceeded);
    assert!(!format!("{error:?}").contains("filename-sentinel"));

    let exact_content_type = format!(
        "application/{}",
        "a".repeat(MAX_MULTIPART_CONTENT_TYPE_BYTES - "application/".len())
    );
    assert_eq!(exact_content_type.len(), MAX_MULTIPART_CONTENT_TYPE_BYTES);
    text("x")
        .content_type(exact_content_type.as_str())
        .expect("exact content-type limit");
    let excessive_content_type = format!("{exact_content_type}a");
    let error = text("x")
        .content_type(excessive_content_type.as_str())
        .expect_err("content-type first excess");
    assert_eq!(error.kind(), BodyErrorKind::LimitExceeded);
}

#[test]
fn aggregate_metadata_exact_limit_and_first_excess_are_incremental() {
    let name = "n".repeat(MAX_MULTIPART_NAME_BYTES);
    let filename = "f".repeat(MAX_MULTIPART_FILENAME_BYTES);
    let mut body = MultipartBody::new();
    for _ in 0..51 {
        body.push_part(
            name.as_str(),
            text("").file_name(filename.as_str()).expect("filename"),
        )
        .expect("aggregate metadata below limit");
    }
    body.push_part(name.as_str(), text(""))
        .expect("exact aggregate metadata limit");
    assert_eq!(body.metadata_bytes, MAX_MULTIPART_METADATA_BYTES);
    let part_count = body.parts.len();

    let error = body
        .push_part("x", text("first-excess-metadata-sentinel"))
        .expect_err("aggregate metadata first excess");
    assert_eq!(error.kind(), BodyErrorKind::LimitExceeded);
    assert_eq!(body.metadata_bytes, MAX_MULTIPART_METADATA_BYTES);
    assert_eq!(body.parts.len(), part_count);
    assert!(!format!("{error:?}").contains("first-excess-metadata-sentinel"));
}

#[test]
fn inline_text_exact_limit_and_first_excess_are_redacted() {
    let exact = "x".repeat(MAX_MULTIPART_INLINE_TEXT_BYTES);
    let part = MultipartPart::text(exact.as_str()).expect("exact inline-text limit");
    assert_eq!(
        part.lower_bound().expect("text lower bound"),
        MAX_MULTIPART_INLINE_TEXT_BYTES as u64
    );
    drop(part);
    drop(exact);

    let excessive = "x".repeat(MAX_MULTIPART_INLINE_TEXT_BYTES + 1);
    let error = MultipartPart::text(excessive.as_str()).expect_err("inline-text first excess");
    assert_eq!(error.kind(), BodyErrorKind::LimitExceeded);
    assert!(!format!("{error:?}").contains("xxxx"));
}

#[test]
fn factory_output_is_revalidated_before_native_construction() {
    let error = PreparedBody::multipart_factory(InvalidMultipartFactory)
        .expect("factory boundary")
        .into_managed(NonZeroUsize::new(1_024).expect("positive limit"))
        .err()
        .expect("invalid factory recipe");
    assert_eq!(error.kind(), BodyErrorKind::LimitExceeded);
    assert!(!format!("{error:?}").contains("factory-name-sentinel"));
}

#[test]
fn replayability_and_framed_lengths_follow_parts() {
    let boundary = MultipartBoundary::generate();
    let blen = boundary.as_str().len() as u64;
    let buffered = MultipartBody::new()
        .part("text", text("four"))
        .expect("part")
        .part(
            "bytes",
            MultipartPart::bytes(Bytes::from_static(b"three")).expect("bounded bytes"),
        )
        .expect("part");
    let framing = buffered.framing(&boundary).expect("framing");
    assert!(framing.reusable());
    let header_text = part_header("text", &text("four")).len() as u64;
    let header_bytes = part_header(
        "bytes",
        &MultipartPart::bytes(Bytes::from_static(b"three")).expect("bytes"),
    )
    .len() as u64;
    let expected = (2 + blen + 2 + header_text + 4 + 4 + 2)
        + (2 + blen + 2 + header_bytes + 4 + 5 + 2)
        + (2 + blen + 4);
    assert_eq!(framing.length_hint(), LengthHint::Exact(expected));
    assert!(
        buffered
            .clone_reusable()
            .expect("valid reusable recipe")
            .is_some()
    );

    let streamed = MultipartBody::new()
        .part(
            "stream",
            MultipartPart::stream(RequestStream::from_stream(futures_stream(vec![
                b"data".to_vec(),
            ]))),
        )
        .expect("part");
    let framing = streamed.framing(&boundary).expect("framing");
    assert!(!framing.reusable());
    assert!(matches!(framing.length_hint(), LengthHint::AtLeast(_)));
    assert!(
        streamed
            .clone_reusable()
            .expect("valid streaming recipe")
            .is_none()
    );
}

#[tokio::test]
async fn serialization_uses_one_boundary_and_exactly_matches_the_framed_length() {
    let boundary = MultipartBoundary::generate();
    let body = MultipartBody::new()
        .part(
            "field one",
            text("alpha").content_type("text/plain").expect("media"),
        )
        .expect("part")
        .part(
            "upload",
            MultipartPart::bytes(Bytes::from_static(b"\x00\x01binary"))
                .expect("bytes")
                .file_name("a\"b\\c.bin")
                .expect("filename"),
        )
        .expect("part");
    let framing = body.framing(&boundary).expect("framing");
    let serialized = serialize(body, &boundary).await.expect("serialized");
    assert_eq!(
        framing.length_hint(),
        LengthHint::Exact(serialized.len() as u64)
    );
    let b = boundary.as_str();
    let expected = format!(
        "--{b}\r\nContent-Disposition: form-data; name*=utf-8''field%20one\r\nContent-Type: text/plain\r\n\r\nalpha\r\n--{b}\r\nContent-Disposition: form-data; name=\"upload\"; filename=\"a\\\"b\\\\c.bin\"\r\n\r\n\x00\x01binary\r\n--{b}--\r\n"
    );
    assert_eq!(serialized, expected.as_bytes());
    assert_eq!(
        boundary.content_type().expect("content type"),
        HeaderValue::from_str(&format!("multipart/form-data; boundary={b}")).expect("header")
    );
}

#[tokio::test]
async fn streaming_part_length_is_enforced_at_its_part_boundary() {
    let boundary = MultipartBoundary::generate();
    let short = MultipartBody::new()
        .part(
            "stream",
            MultipartPart::stream(
                RequestStream::from_stream(futures_stream(vec![b"abc".to_vec()])).exact_length(4),
            ),
        )
        .expect("part");
    let error = serialize(short, &boundary)
        .await
        .expect_err("truncated streaming part");
    assert_eq!(error.kind(), BodyErrorKind::InvalidLength);

    let long = MultipartBody::new()
        .part(
            "stream",
            MultipartPart::stream(
                RequestStream::from_stream(futures_stream(vec![b"abcde".to_vec()])).exact_length(4),
            ),
        )
        .expect("part");
    let error = serialize(long, &boundary)
        .await
        .expect_err("excess streaming part");
    assert_eq!(error.kind(), BodyErrorKind::InvalidLength);

    let below_minimum = MultipartBody::new()
        .part(
            "stream",
            MultipartPart::stream(
                RequestStream::from_stream(futures_stream(vec![b"ab".to_vec()])).at_least(3),
            ),
        )
        .expect("part");
    let error = serialize(below_minimum, &boundary)
        .await
        .expect_err("dishonest minimum");
    assert_eq!(error.kind(), BodyErrorKind::InvalidLength);
}

#[test]
fn boundary_is_valid_bounded_and_regenerated_away_from_buffered_collisions() {
    for _ in 0..16 {
        let boundary = MultipartBoundary::generate();
        assert!(valid_boundary(boundary.as_str()));
        assert!(boundary.as_str().len() <= MAX_BOUNDARY_BYTES);
        assert!(!format!("{boundary:?}").contains(boundary.as_str()));
    }
    let first = MultipartBoundary::generate();
    let second = MultipartBoundary::generate();
    assert_ne!(first.as_str(), second.as_str());

    let colliding = MultipartBody::new()
        .part(
            "field",
            text(&format!("prefix --{} suffix", first.as_str())),
        )
        .expect("part");
    assert!(colliding.contains_delimiter(&first));
    let selected = MultipartBoundary::generate_for(&colliding).expect("fresh boundary");
    assert!(!colliding.contains_delimiter(&selected));
}

struct VecStream(std::collections::VecDeque<Vec<u8>>);

impl futures_core::Stream for VecStream {
    type Item = Result<Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(self.0.pop_front().map(|chunk| Ok(Bytes::from(chunk))))
    }
}

fn futures_stream(chunks: Vec<Vec<u8>>) -> VecStream {
    VecStream(chunks.into())
}
