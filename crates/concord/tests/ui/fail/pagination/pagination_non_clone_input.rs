// §9.3: `PageRequestRepresentation<Input>` carries an actual `Input: Clone`
// obligation; a representation's conditional page promise does not apply to
// a non-Clone body input.
use std::marker::PhantomData;

use concord::body::PageRequestRepresentation;
use concord::codec::{CodecError, CodecErrorKind, DecodeBody, DecodeContext, EncodeBody, EncodeContext};
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

pub struct NonCloneInput;

pub struct ConditionalPageCodec<T>(PhantomData<fn() -> T>);

impl<T> EncodeBody for ConditionalPageCodec<T>
where
    T: Send + 'static,
{
    type Input = T;

    const CONTENT_TYPE: Option<&'static str> = None;

    fn encode(_input: Self::Input, _context: EncodeContext<'_>) -> Result<Bytes, CodecError> {
        Ok(Bytes::new())
    }
}

impl<T> PageRequestRepresentation<T> for ConditionalPageCodec<T> where T: Clone + Send + 'static {}

pub const PAGE_SIZE: std::num::NonZeroU32 = std::num::NonZeroU32::MIN;

api! {
    client NonClonePageInputApi {
        base "https://example.test";

        POST read(
            body input: crate::NonCloneInput via crate::ConditionalPageCodec<crate::NonCloneInput>,
            query offset: u64 = 0,
        ) -> Vec<String> via crate::Lines {
            effect safe;
            paginate via OffsetPagination {
                init OffsetInit { offset: offset, page_size: crate::PAGE_SIZE };
                cursor -> offset;
            }
        }
    }
}

fn main() {}
