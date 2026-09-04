use concord_core::contracts::{
    BodyError, BodyKind, DiscardDriver, EndpointId, LengthHint, Method, RequestBody,
    RequestBodyContext, RequestBodyMetadata, RequestContentType, RequestRepresentation,
    ResponseBodyError, ResponseHead, ResponseRepresentation, RuntimeEndpoint,
    RuntimeEndpointBuilder,
};

struct GenericNull;

impl<T> RequestRepresentation<T> for GenericNull
where
    T: Send + 'static,
{
    fn metadata(
        _input: &T,
        _context: RequestBodyContext<'_>,
    ) -> Result<RequestBodyMetadata, BodyError> {
        Ok(RequestBodyMetadata {
            length: LengthHint::Exact(0),
            content_type: RequestContentType::None,
            kind: BodyKind::Empty,
        })
    }

    fn prepare(_input: T, _context: RequestBodyContext<'_>) -> Result<RequestBody, BodyError> {
        Ok(RequestBody::empty())
    }
}

impl<T> ResponseRepresentation<T> for GenericNull
where
    T: Default + Send + 'static,
{
    type Driver = DiscardDriver;

    fn from_body((): (), _head: &ResponseHead<'_>) -> Result<T, ResponseBodyError> {
        Ok(T::default())
    }
}

#[test]
fn one_zero_sized_marker_can_implement_multiple_exact_pairs_without_native_authority() {
    fn accepts_request_pair<R, Input>()
    where
        R: RequestRepresentation<Input>,
        Input: Send + 'static,
    {
    }

    fn accepts_response_pair<R, Output>()
    where
        R: ResponseRepresentation<Output>,
        Output: Send + 'static,
    {
    }

    accepts_request_pair::<GenericNull, String>();
    accepts_request_pair::<GenericNull, Vec<u8>>();
    accepts_response_pair::<GenericNull, String>();
    accepts_response_pair::<GenericNull, Vec<u8>>();

    let runtime: RuntimeEndpointBuilder<GenericNull, String> = RuntimeEndpoint::builder(
        EndpointId::new("representation_api.generic_null").expect("endpoint ID"),
        Method::POST,
    )
    .body_spec::<GenericNull, _>(vec![1_u8, 2, 3])
    .response::<GenericNull, String>();
    let _ = runtime.build().expect("generic representation pair");
}
