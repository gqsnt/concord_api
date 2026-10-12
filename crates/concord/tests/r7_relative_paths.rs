#![allow(dead_code)]

mod outer {
    pub const DEFAULT_COUNT: u32 = 7;
    pub type Count = u32;
    pub type Wire = concord::Bytes;
    pub use concord::Binary as WireCodec;
    pub struct PathProvider;
    impl concord::auth::CredentialProvider for PathProvider {
        type Material = concord::auth::AccessToken;
        fn acquire<'a>(
            &'a self,
            _: concord::auth::CredentialRequest<'a>,
        ) -> concord::auth::BoxFuture<'a, Result<Self::Material, concord::auth::AuthError>>
        {
            Box::pin(async {
                Ok(concord::auth::AccessToken::try_new("relative-provider-token").unwrap())
            })
        }
    }
    #[cfg(feature = "json")]
    pub use concord::pagination::{OffsetInit, OffsetPagination};
    pub const FIRST_OFFSET: u64 = 2;
    pub mod inner {
        pub const DEFAULT_COUNT: u64 = 9;
        pub type Count = u64;
        pub type Wire = String;
        use super::Count as ImportedCount;
        pub use concord::Empty as UnitCodec;
        pub struct PathProvider;
        pub const FIRST_OFFSET: u64 = 99;
        concord::api! {
            client RelativeApi {
                base "http://review.example.test/";
                GET read(query count: u32 = super::DEFAULT_COUNT) -> () via concord::Empty;
                GET lexical(
                    query a: self::Count = self::DEFAULT_COUNT,
                    query b: super::Count = super::DEFAULT_COUNT,
                    query c: ImportedCount = crate::ROOT_COUNT,
                    query d: ::std::primitive::u32 = super::super::ROOT_COUNT,
                ) -> () via self::UnitCodec;
                POST body(body input: super::Wire via super::WireCodec) -> super::Wire via super::WireCodec;
                scope nested() {
                    scope deeper() {
                        GET read(query count: u32 = super::DEFAULT_COUNT) -> () via concord::Empty;
                        POST body(body input: super::Wire via super::WireCodec) -> super::Wire via super::WireCodec;
                        GET nested(query("x") each xs: Vec<super::Count>) -> () via self::UnitCodec;
                    }
                }
            }
        }
        concord::api! {
            client ProviderPaths {
                base "http://provider-paths.example.test/";
                credentials { credential token = custom super::PathProvider; }
                GET read() -> () via self::UnitCodec { auth { bearer token; } }
            }
        }
        #[cfg(feature = "json")]
        concord::api! {
            client PaginationPaths {
                base "http://pagination-paths.example.test/";
                GET read(query offset: u64 = super::FIRST_OFFSET, query count: ::std::num::NonZeroU32 = ::std::num::NonZeroU32::MIN) -> Vec<String> via concord::Json {
                    paginate via super::OffsetPagination {
                        init super::OffsetInit { offset: super::FIRST_OFFSET, page_size: count };
                        cursor -> offset;
                    }
                }
            }
        }
    }
}

#[test]
fn r7_provider_type_keeps_its_parent_identity() {
    let api = outer::inner::ProviderPaths::builder()
        .token(outer::PathProvider)
        .build()
        .unwrap();
    let _ = api.read(); // Constructor requires the exact parent provider type.
}

#[cfg(feature = "json")]
#[tokio::test]
async fn r7_pagination_initializer_type_and_value_keep_parent_bindings() {
    use concord_test_support::server::{ResponseScript, ScriptedServer};
    let server = ScriptedServer::start([ResponseScript::Complete {
        status: 200,
        headers: vec![("Content-Type".into(), "application/json".into())],
        body: b"[]".to_vec(),
    }])
    .unwrap();
    let proxy = concord::http::SafeProxy::parse(server.url("")).unwrap();
    let api = outer::inner::PaginationPaths::builder()
        .configure_http(move |http| http.proxy(proxy))
        .build()
        .unwrap();
    let mut pages = api.read().pages(1);
    assert!(pages.next_page().await.unwrap().is_some());
    assert!(server.requests()[0].target().contains("offset=2"));
}

pub const ROOT_COUNT: u32 = 11;

#[test]
fn r7_authored_types_and_constants_keep_lexical_identity_at_each_generated_use() {
    fn bytes_output<
        F: std::future::IntoFuture<Output = Result<concord::Bytes, concord::ApiError>>,
    >(
        _: F,
    ) {
    }
    let api = outer::inner::RelativeApi::new().unwrap();
    bytes_output(api.body(concord::Bytes::new()));
    bytes_output(api.nested().deeper().body(concord::Bytes::new()));
    let plan = api.lexical().plan().unwrap();
    assert!(plan.observed_url().ends_with("?a=9&b=7&c=11&d=11"));
    let plan = api.nested().deeper().nested(vec![7_u32]).plan().unwrap();
    assert!(plan.observed_url().ends_with("?x=7"));
}

#[test]
fn r7_authored_parent_constant_is_preserved_in_root_and_nested_scopes() {
    let api = outer::inner::RelativeApi::new().unwrap();
    assert!(
        api.read()
            .plan()
            .unwrap()
            .observed_url()
            .ends_with("?count=7")
    );
    assert!(
        api.nested()
            .deeper()
            .read()
            .plan()
            .unwrap()
            .observed_url()
            .ends_with("?count=7")
    );
}
