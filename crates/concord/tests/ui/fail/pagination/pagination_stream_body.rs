// §9.3: one-shot streams do not obtain the page-body promise by default.
use concord::codec::{CodecError, CodecErrorKind, DecodeBody, DecodeContext};
use concord::pagination::{OffsetInit, OffsetPagination};
use concord::{Binary, Bytes, RequestStream, api};

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
    client StreamPageBodyApi {
        base "https://example.test";

        POST read(body upload: RequestStream via Binary, query offset: u64 = 0) -> Vec<String> via crate::Lines {
            effect safe;
            paginate via OffsetPagination {
                init OffsetInit { offset: offset, page_size: crate::PAGE_SIZE };
                cursor -> offset;
            }
        }
    }
}

fn main() {}
