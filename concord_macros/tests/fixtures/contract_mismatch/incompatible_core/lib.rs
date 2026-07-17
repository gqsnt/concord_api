#![forbid(unsafe_code)]

#[doc(hidden)]
pub mod __private {
    #[doc(hidden)]
    pub const GENERATED_CONTRACT_VERSION: u32 = 999;

    #[doc(hidden)]
    pub const fn assert_generated_contract(expected: u32) {
        assert!(
            GENERATED_CONTRACT_VERSION == expected,
            "concord macro/core contract mismatch"
        );
    }

    pub struct ApiError;
    pub struct BodyMetadata;
    pub trait BodySpec: Send + 'static {}
    pub struct EmptyBody;
    impl EmptyBody {
        pub const fn new() -> Self {
            Self
        }

        pub const fn metadata() -> BodyMetadata {
            BodyMetadata
        }
    }
    impl BodySpec for EmptyBody {}

    pub trait ResponseHandler: Send + Sync + 'static {
        type Output: Send + 'static;
    }
    pub struct NoContentResponse;
    impl ResponseHandler for NoContentResponse {
        type Output = ();
    }

    pub struct Method;
    impl Method {
        pub const GET: Self = Self;
    }
    pub struct RequestMeta;
    impl RequestMeta {
        pub const fn new(
            _endpoint: &'static str,
            _method: Method,
            _timeout: Option<std::time::Duration>,
        ) -> Self {
            Self
        }
    }
    pub struct PreparedHead;
    pub struct HeadBuilder;
    impl HeadBuilder {
        pub fn push_static(&mut self, _segment: &str) -> Result<(), ApiError> {
            Ok(())
        }

        pub fn push_value<T: std::fmt::Display + ?Sized>(
            &mut self,
            _value: &T,
        ) -> Result<(), ApiError> {
            Ok(())
        }

        pub fn finish(self) -> PreparedHead {
            PreparedHead
        }
    }

    pub trait EndpointSpec: Send + 'static {
        type Body: BodySpec;
        type Response: ResponseHandler;

        fn request_meta(&self) -> RequestMeta;
        fn prepare_head_and_body_metadata(
            &self,
            builder: HeadBuilder,
        ) -> Result<(PreparedHead, BodyMetadata), ApiError>;
        fn into_body(self) -> Self::Body;
    }
}
