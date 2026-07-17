mod crate_root_paths {
    use concord_core::{Request, ResponseRequest};

    pub fn names_are_available<E>(
        request: Option<Request<E>>,
        response: Option<ResponseRequest<E>>,
    ) -> (Option<Request<E>>, Option<ResponseRequest<E>>) {
        (request, response)
    }
}

mod prelude_paths {
    use concord_core::prelude::{Request, ResponseRequest};

    pub fn names_are_available<E>(
        request: Option<Request<E>>,
        response: Option<ResponseRequest<E>>,
    ) -> (Option<Request<E>>, Option<ResponseRequest<E>>) {
        (request, response)
    }
}

#[test]
fn stable_wrapper_paths_are_publicly_nameable() {
    let (request, response) = crate_root_paths::names_are_available::<()>(None, None);
    assert!(request.is_none());
    assert!(response.is_none());
    let (request, response) = prelude_paths::names_are_available::<()>(None, None);
    assert!(request.is_none());
    assert!(response.is_none());
}
