use concord::api;

#[path = "../../../support/mod.rs"]
mod support;

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
    client MultipartPageBodyApi {
        timeout 30s;
        base "https://example.test";


    POST read(
        body body: ::concord::body::MultipartBody via ::concord::body::Multipart,
    ) -> Vec<u8> via crate::support::Json {
        effect safe;
        paginate via crate::Controller {}
    }
}}

fn main() {}
