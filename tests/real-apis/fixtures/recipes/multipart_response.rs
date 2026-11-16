use concord::ByteStream;
use std::io;

/// Application errors from either Concord or the MIME parser.
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// Opens a finite multipart response under a bound on all its wire bytes.
pub fn parts(body: ByteStream, max_bytes: u64) -> Result<multer::Multipart<'static>, BoxError> {
    let label: mime::Mime = body
        .headers()
        .get("content-type")
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Missing multipart content type.",
            )
        })?
        .parse()?;
    if label.type_() != mime::MULTIPART {
        return Err(
            io::Error::new(io::ErrorKind::InvalidData, "Expected a multipart response.").into(),
        );
    }
    // multer::parse_boundary accepts form-data only; mixed and related need this parameter.
    let boundary = label
        .get_param(mime::BOUNDARY)
        .filter(|value| !value.as_str().is_empty() && value.as_str().len() <= 70)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "Missing or invalid MIME boundary.",
            )
        })?
        .as_str()
        .to_owned();
    let limits = multer::Constraints::new().size_limit(
        multer::SizeLimit::new()
            .whole_stream(max_bytes)
            .per_field(256 * concord::KIB as u64),
    );
    Ok(multer::Multipart::with_constraints(body, boundary, limits))
}
