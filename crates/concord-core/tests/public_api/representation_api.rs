use std::num::NonZeroUsize;

use concord_core::body::{
    BodyDescription, BodyError, BodyErrorKind, PreparedBody, RequestBodyContext,
    RequestRepresentation,
};
use concord_core::contracts::{
    BodyKind, Bytes, DiscardDriver, EndpointId, LengthHint, Method, ResponseBodyError,
    ResponseHead, ResponseRepresentation, RuntimeEndpoint, RuntimeEndpointBuilder,
};

struct GenericNull;

impl<T> RequestRepresentation<T> for GenericNull
where
    T: Send + 'static,
{
    const DESCRIPTION: BodyDescription = BodyDescription::EMPTY;

    fn prepare(_input: T, _context: RequestBodyContext<'_>) -> Result<PreparedBody, BodyError> {
        Ok(PreparedBody::empty())
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
    .body_repr::<GenericNull, _>(vec![1_u8, 2, 3])
    .response::<GenericNull, String>();
    let endpoint = runtime.build().expect("generic representation pair");
    assert_eq!(
        endpoint.description().body_description(),
        BodyDescription::EMPTY
    );
}

// Spec §17.3: a downstream custom body representation with one authoritative
// preparation and no duplicated Content-Type method.
pub struct GreetingBody;
pub struct Greeting {
    pub text: String,
}

impl RequestRepresentation<Greeting> for GreetingBody {
    fn prepare(
        input: Greeting,
        context: RequestBodyContext<'_>,
    ) -> Result<PreparedBody, BodyError> {
        let bytes = input.text.into_bytes();
        if bytes.len() > context.body_limit.get() {
            return Err(BodyError::new(BodyErrorKind::LimitExceeded));
        }
        PreparedBody::bytes(Bytes::from(bytes)).content_type("text/plain; charset=utf-8")
    }
}

#[test]
fn downstream_custom_body_owns_one_exact_content_type() {
    let endpoint = EndpointId::new("representation_api.greeting").expect("endpoint ID");
    let context = RequestBodyContext::new(
        &endpoint,
        &Method::POST,
        NonZeroUsize::new(16).expect("positive limit"),
        None,
    );
    let body = GreetingBody::prepare(
        Greeting {
            text: "hello".to_owned(),
        },
        context,
    )
    .expect("greeting body");
    assert_eq!(body.kind(), BodyKind::Buffered);
    assert_eq!(body.media_type(), Some("text/plain; charset=utf-8"));
    assert_eq!(body.length_hint(), LengthHint::Exact(5));
    assert!(body.is_replayable());
    assert_eq!(
        <GreetingBody as RequestRepresentation<Greeting>>::DESCRIPTION,
        BodyDescription::UNKNOWN
    );

    let error = GreetingBody::prepare(
        Greeting {
            text: "x".repeat(17),
        },
        context,
    )
    .expect_err("limit");
    assert_eq!(error.kind(), BodyErrorKind::LimitExceeded);

    let conflicting = PreparedBody::bytes(Bytes::from_static(b"x"))
        .content_type("text/plain")
        .expect("first media type")
        .content_type("application/json")
        .expect_err("conflicting media type");
    assert_eq!(conflicting.kind(), BodyErrorKind::InvalidContentType);
    PreparedBody::bytes(Bytes::from_static(b"x"))
        .content_type("text/plain")
        .expect("first media type")
        .content_type("text/plain")
        .expect("identical repeat is harmless");
}
