// §14.2 / §25.2: `.paginate(PageLimit)` is replaced by distinct terminals;
// `collect_all` takes a `NonZeroU32` bound (no zero-page "complete"
// collection), and no runtime enum selects unrelated return types.
use std::num::NonZeroU32;

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

pub const PAGE_SIZE: NonZeroU32 = NonZeroU32::MIN;

api! {
    client ExplicitPageLimitApi {
        base "https://example.test";

        GET read(query offset: u64 = 0) -> Vec<String> via crate::Lines {
            paginate via OffsetPagination {
                init OffsetInit { offset: offset, page_size: crate::PAGE_SIZE };
                cursor -> offset;
            }
        }
    }
}

fn main() {
    let client = ExplicitPageLimitApi::new().unwrap();
    let _ = client.read().collect_all(10_u32);
    let _ = client.read().paginate(10_u32);
}
