// §9.3 / §14.1: `Clone` alone does not implement the page-body promise; a
// request representation must opt into `PageRequestRepresentation`
// explicitly before a body-bearing endpoint can page.
use concord::body::{BodyError, PreparedBody, RequestBodyContext, RequestRepresentation};
use concord::codec::{CodecError, CodecErrorKind, DecodeBody, DecodeContext};
use concord::pagination::{OffsetInit, OffsetPagination};
use concord::{Bytes, api};

pub struct Lines;

impl DecodeBody for Lines {
    type Output = Vec<String>;

    const ACCEPT: Option<&'static str> = Some("text/plain");

    fn decode(body: Bytes, _context: DecodeContext<'_>) -> Result<Self::Output, CodecError> {
        let text = String::from_utf8(body.to_vec()).map_err(|_| CodecError::new(CodecErrorKind::Decode))?;
        Ok(text.lines().map(str::to_owned).collect())
    }
}

pub struct NonPageRepresentation;

impl RequestRepresentation<String> for NonPageRepresentation {
    fn prepare(input: String, _context: RequestBodyContext<'_>) -> Result<PreparedBody, BodyError> {
        PreparedBody::bytes(Bytes::from(input)).content_type("text/plain")
    }
}

pub const PAGE_SIZE: std::num::NonZeroU32 = std::num::NonZeroU32::MIN;

api! {
    client NonRepeatableCodecApi {
        base "https://example.test";

        POST read(body query: String via crate::NonPageRepresentation, query offset: u64 = 0) -> Vec<String> via crate::Lines {
            effect safe;
            paginate via OffsetPagination {
                init OffsetInit { offset: offset, page_size: crate::PAGE_SIZE };
                cursor -> offset;
            }
        }
    }
}

// An optional body needs the static page-body capability even when absent.
api! {
    client OptionalNonPageBodyApi {
        base "https://example.test";

        POST read(body query?: String via crate::NonPageRepresentation, query offset: u64 = 0) -> Vec<String> via crate::Lines {
            effect safe;
            paginate via OffsetPagination {
                init OffsetInit { offset: offset, page_size: crate::PAGE_SIZE };
                cursor -> offset;
            }
        }
    }
}

fn main() {}
