// BODY-02 / §14.1: a body-bearing page request needs an explicit
// `PageRequestRepresentation` promise over a `Clone` input; arbitrary
// multipart producers do not obtain it.
use concord::body::{Multipart, MultipartBody};
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

pub const PAGE_SIZE: std::num::NonZeroU32 = std::num::NonZeroU32::MIN;

api! {
    client MultipartPageBodyApi {
        base "https://example.test";

        POST read(body form: MultipartBody via Multipart, query offset: u64 = 0) -> Vec<String> via crate::Lines {
            effect safe;
            paginate via OffsetPagination {
                init OffsetInit { offset: offset, page_size: crate::PAGE_SIZE };
                cursor -> offset;
            }
        }
    }
}

fn main() {}
