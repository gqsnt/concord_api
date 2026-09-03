mod crate_root_paths {
    use concord_core::{AcceptedBytesRequest, Request, ResponseRequest};

    type RequestVariants<E> = (
        Option<Request<E>>,
        Option<ResponseRequest<E>>,
        Option<AcceptedBytesRequest<E>>,
    );

    pub fn names_are_available<E>(
        request: Option<Request<E>>,
        response: Option<ResponseRequest<E>>,
        accepted_bytes: Option<AcceptedBytesRequest<E>>,
    ) -> RequestVariants<E> {
        (request, response, accepted_bytes)
    }
}

#[test]
fn stable_wrapper_paths_are_publicly_nameable() {
    let (request, response, accepted_bytes) =
        crate_root_paths::names_are_available::<()>(None, None, None);
    assert!(request.is_none());
    assert!(response.is_none());
    assert!(accepted_bytes.is_none());
}
