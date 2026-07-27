mod crate_root_paths {
    use concord_core::{Request, ResponseBytesRequest, ResponseRequest};

    pub fn names_are_available<E>(
        request: Option<Request<E>>,
        response: Option<ResponseRequest<E>>,
        response_bytes: Option<ResponseBytesRequest<E>>,
    ) -> (
        Option<Request<E>>,
        Option<ResponseRequest<E>>,
        Option<ResponseBytesRequest<E>>,
    ) {
        (request, response, response_bytes)
    }
}

#[test]
fn stable_wrapper_paths_are_publicly_nameable() {
    let (request, response, response_bytes) =
        crate_root_paths::names_are_available::<()>(None, None, None);
    assert!(request.is_none());
    assert!(response.is_none());
    assert!(response_bytes.is_none());
}
