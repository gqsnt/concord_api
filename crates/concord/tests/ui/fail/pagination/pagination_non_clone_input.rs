use std::marker::PhantomData;

use concord::api;
use concord::body::PageRequestRepresentation;
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

impl<T> PageRequestRepresentation<T> for ConditionalPageCodec<T>
where
    T: Clone + Send + 'static,
{
}

pub struct Controller {}

impl<Page> concord::pagination::PaginationController<Page> for Controller
where
    Page: Send + 'static,
{
    type Item = ();

    const DESCRIPTION: concord::pagination::PaginationDescription =
        concord::pagination::PaginationDescription::NONE;

    fn advance(
        self,
        _page: concord::pagination::PageResponse<Page>,
        _limits: concord::pagination::PageDecisionLimits,
    ) -> Result<
        concord::pagination::PageStep<Self, Self::Item>,
        concord::pagination::PaginationError,
    > {
        Ok(concord::pagination::PageStep::Done { items: Vec::new() })
    }
}

api! {
    client NonClonePageInputApi {
        timeout 30s;
        base "https://example.test";


    POST read(
        body body: crate::NonCloneInput via crate::ConditionalPageCodec<crate::NonCloneInput>,
    ) -> Vec<u8> via crate::support::Json {
        effect safe;
        paginate via crate::Controller {}
    }
}}

fn main() {}
