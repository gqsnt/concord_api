mod crate_root_paths {
    use concord_core::{AcceptedBytesRequest, Request, ResponseRequest};

    type RequestVariants<E, O> = (
        Option<Request<E>>,
        Option<ResponseRequest<O>>,
        Option<AcceptedBytesRequest>,
    );

    /// `ResponseRequest` names only the declared output (§7.4); the
    /// endpoint type is erased. `AcceptedBytesRequest` names nothing.
    pub fn names_are_available<E, O>(
        request: Option<Request<E>>,
        response: Option<ResponseRequest<O>>,
        accepted_bytes: Option<AcceptedBytesRequest>,
    ) -> RequestVariants<E, O> {
        (request, response, accepted_bytes)
    }
}

#[test]
fn stable_wrapper_paths_are_publicly_nameable() {
    let (request, response, accepted_bytes) =
        crate_root_paths::names_are_available::<(), String>(None, None, None);
    assert!(request.is_none());
    assert!(response.is_none());
    assert!(accepted_bytes.is_none());
}

#[test]
fn erased_terminals_are_send() {
    fn assert_send<T: Send>() {}
    assert_send::<ResponseRequestOf<String>>();
    assert_send::<concord_core::AcceptedBytesRequest>();
}

type ResponseRequestOf<O> = concord_core::ResponseRequest<O>;
