use concord::api;

#[path = "../../../support/mod.rs"]
mod support;

pub struct NonPageRepresentation;

impl concord::body::RequestRepresentation<String> for NonPageRepresentation {
    fn metadata(
        input: &String,
        context: concord::body::RequestBodyContext<'_>,
    ) -> Result<concord::body::RequestBodyMetadata, concord::body::BodyError> {
        <concord::Text as concord::body::RequestRepresentation<String>>::metadata(input, context)
    }

    fn prepare(
        input: String,
        context: concord::body::RequestBodyContext<'_>,
    ) -> Result<concord::body::RequestBody, concord::body::BodyError> {
        <concord::Text as concord::body::RequestRepresentation<String>>::prepare(input, context)
    }
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
    client NonRepeatableCodecApi {
        timeout 30s;
        base "https://example.test";


    POST read(
        body body: String via crate::NonPageRepresentation,
    ) -> Vec<u8> via crate::support::Json {
        effect safe;
        paginate via crate::Controller {}
    }
}}

fn main() {}

