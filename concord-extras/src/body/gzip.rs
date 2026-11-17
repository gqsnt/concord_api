//! Gzip request bodies made from buffered bodies.
use concord_core::Body;
use std::{error::Error, fmt, io::Read};

/// Compresses a buffered body as one deterministic gzip member, keeping its content type.
///
/// An input with no content type produces a body with no content type.
/// The same bytes give the same output: the gzip header contains no timestamp,
/// filename or comment. The result is bytes and can be sent again by the engine.
/// The compressed length is the output length; do not retain a length from the
/// uncompressed body. Encoder working memory depends on its selected backend.
/// No stream is collected or polled, and no request or task is started.
///
/// The description sets `Content-Encoding: gzip`: a writer returns a `Body`,
/// which carries no request header except its content type.
/// A description's `content-type` header takes precedence.
///
/// This writer needs the feature `gzip-body`. The feature `gzip` is response
/// decompression, in `reqwest`; it does not provide this writer. The example below
/// needs the features `gzip-body` and `json`.
///
/// ```no_run
/// concord::api! {
///     client Logs {
///         base "https://logs.example.test";
///         POST write(events: Vec<String>) {
///             headers { "content-encoding" = "gzip"; }
///             body concord::gzip(concord::json(events)?);
///         }
///     }
/// }
/// async fn write() -> Result<(), concord::Error> {
///     Logs::new().write(vec!["ready".into()]).await?;
///     Ok(())
/// }
/// ```
///
/// # Errors
///
/// Returns [`GzipError::Stream`] for a body not held as bytes, without claiming
/// or polling any source.
pub fn gzip(body: impl Into<Body>) -> Result<Body, GzipError> {
    let body = body.into();
    let bytes = body.as_bytes().ok_or(GzipError::Stream)?;
    let mut encoded = Vec::new();
    let mut encoder = flate2::GzBuilder::new()
        .mtime(0)
        .operating_system(255)
        .buf_read(bytes, flate2::Compression::new(6));
    // The reader is a byte slice and the output is a Vec, so neither can fail
    // with an I/O error. A defensive stored-member fallback also keeps an
    // unexpected codec failure lossless without inventing a public I/O variant.
    if encoder.read_to_end(&mut encoded).is_err() {
        encoded = stored_member(bytes);
    }
    let mut output = Body::from(encoded);
    if let Some(content_type) = body.get_content_type() {
        output = output.content_type(content_type.to_owned());
    }
    Ok(output)
}

/// A request body that cannot be compressed without consuming a stream.
///
/// Formatting includes no bytes and no content type. It has no lower-level cause.
///
/// ```no_run
/// fn inspect(error: &concord::Error) {
///     if let Some(concord::GzipError::Stream) = concord::GzipError::find(error) {
///         // Supply a buffered body or the application's streaming writer.
///     }
/// }
/// ```
#[non_exhaustive]
#[derive(Debug)]
pub enum GzipError {
    /// The body is a stream; no source was claimed or polled.
    Stream,
}
impl GzipError {
    /// Finds a `GzipError` in this error or its chain of sources.
    ///
    /// ```no_run
    /// fn inspect(error: &concord::Error) {
    ///     let _cause = concord::GzipError::find(error);
    /// }
    /// ```
    pub fn find<'a>(error: &'a (dyn Error + 'static)) -> Option<&'a Self> {
        super::find_error(error)
    }
}
impl fmt::Display for GzipError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("the gzip body must be buffered")
    }
}
impl Error for GzipError {}

// RFC 1951 stored blocks are an infallible byte representation for the
// defensive codec-error path. The gzip CRC and size use the same flate2 codec.
fn stored_member(bytes: &[u8]) -> Vec<u8> {
    let mut output = vec![31, 139, 8, 0, 0, 0, 0, 0, 0, 255];
    if bytes.is_empty() {
        output.extend_from_slice(&[1, 0, 0, 255, 255]);
    } else {
        // PANIC-SAFETY: u16::MAX is a nonzero block size.
        let mut blocks = bytes.chunks(usize::from(u16::MAX)).peekable();
        while let Some(block) = blocks.next() {
            output.push(u8::from(blocks.peek().is_none()));
            // PANIC-SAFETY: chunks limits each block length to u16::MAX.
            let length = block.len() as u16;
            output.extend_from_slice(&length.to_le_bytes());
            output.extend_from_slice(&(!length).to_le_bytes());
            output.extend_from_slice(block);
        }
    }
    let mut crc = flate2::Crc::new();
    crc.update(bytes);
    // PANIC-SAFETY: Crc::sum is a checksum accessor, not an iterator's unchecked arithmetic sum.
    output.extend_from_slice(&crc.sum().to_le_bytes());
    output.extend_from_slice(&crc.amount().to_le_bytes());
    output
}
