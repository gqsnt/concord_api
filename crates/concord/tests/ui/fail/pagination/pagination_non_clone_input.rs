use std::marker::PhantomData;

use concord::api;
use concord::body::PageRequestBodySpec;
use concord::codec::{Bytes, CodecError, EncodeBody, EncodeContext};

#[path = "../../../support/mod.rs"]
mod support;

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

impl<T> PageRequestBodySpec for ConditionalPageCodec<T>
where
    T: Clone + Send + 'static,
{
}

pub struct Controller {}

impl<Page> concord::pagination::PaginationController<Page> for Controller {
    fn next(
        self,
        _page: &Page,
    ) -> Result<Option<Self>, concord::pagination::PaginationError> {
        Ok(None)
    }
}

api! {
    client NonClonePageInputApi {
        timeout off;
        base "https://example.test";


    POST read(
        body body: crate::NonCloneInput using crate::ConditionalPageCodec<crate::NonCloneInput>,
    ) -> Vec<u8> using crate::support::Json<Vec<u8>> {
        paginate with crate::Controller {}
    }
}}

fn main() {}
