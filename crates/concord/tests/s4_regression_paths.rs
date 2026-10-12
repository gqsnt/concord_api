#![allow(dead_code, non_camel_case_types)]
mod outer {
    pub type Token = concord::auth::AccessToken;
    pub struct TokenCodec;
    impl concord::response::ResponseRepresentation<Token> for TokenCodec {
        type Driver = concord::response::Buffered;
        fn validate(
            _: &concord::response::ResponseHead<'_>,
        ) -> Result<(), concord::response::ResponseBodyError> {
            Ok(())
        }
        fn from_body(
            _: concord::Bytes,
            _: &concord::response::ResponseHead<'_>,
        ) -> Result<Token, concord::response::ResponseBodyError> {
            Ok(Token::try_new("s4-fixture-token").unwrap())
        }
    }
    pub mod inner {

        concord::api! {
            client CredentialPaths {
                base "https://s4.example.test/";
                credentials { credential session = endpoint nested::deeper::login; }
                scope nested() { scope deeper() { POST login() -> super::Token via super::TokenCodec; } }
                GET read() -> () via concord::Empty { auth { bearer session; } }
            }
        }
    }
}
#[test]
fn endpoint_credential_paths_compile_and_bind() {
    let client = outer::inner::CredentialPaths::new().unwrap();
    client
        .credentials()
        .replace_session(outer::Token::try_new("s4-local-token").unwrap())
        .unwrap();
    let _ = client.nested().deeper().login().store_as_session();
    assert!(client.read().plan().is_ok());
}

pub type ReadRequest = u32;
pub struct CallbackWire;
impl<T> concord::AtomEncoder<fn(T)> for CallbackWire {
    fn write(
        _: &fn(T),
        output: &mut concord::BoundedAtomWriter<'_>,
    ) -> Result<(), concord::AtomError> {
        output.push_str("callback")
    }
}
concord::api! {
    client FunctionPaths {
        base "http://s4.example.test/";
        GET read(query callback: fn(crate::ReadRequest) via crate::CallbackWire) -> () via concord::Empty;
    }
}
fn callback(_: u32) {}
#[test]
fn function_pointer_authored_parameter_binding() {
    let api = FunctionPaths::new().unwrap();
    let request = api.read(callback as fn(ReadRequest));
    assert!(
        request
            .plan()
            .unwrap()
            .observed_url()
            .ends_with("?callback=callback")
    );
}

macro_rules! identity_case {
    ($module:ident,$client:ident,$snake:literal) => {
        mod $module {
            concord::api! { client $client { base "https://fixture.test/"; GET read() -> () via concord::Empty; } }
            #[test]
            fn derived_contract_id_is_the_generated_plan_lookup_id() {
                let expected=concat!($snake,".read");
                assert_eq!($client::contract().endpoints()[0].id(),expected);
                assert!($client::new().unwrap().read().plan().is_ok());
            }
        }
    }
}
identity_case!(underscore, Foo_Bar, "foo_bar");
identity_case!(acronym, HTTPStatus, "http_status");
identity_case!(digit, Api2Client, "api2_client");
identity_case!(digit_acronym, V2API, "v2_api");
identity_case!(trailing_acronym, RiotAPI, "riot_api");
identity_case!(all_acronym, ABC, "abc");
identity_case!(underscore_acronym, Foo_HTTPStatus, "foo_http_status");
identity_case!(raw, r#HTTPStatus, "http_status");

concord::api! { client TimeoutMetadata { base "https://fixture.test/"; GET read() -> () via concord::Empty; } }
#[test]
fn runtime_endpoint_fallback_does_not_rewrite_generated_timeout() {
    let api = TimeoutMetadata::builder()
        .configure_runtime(|runtime| {
            runtime.runtime_endpoint_timeout(std::time::Duration::from_secs(1))
        })
        .build()
        .unwrap();
    assert_eq!(
        api.read().control_summary().declared_timeout(),
        Some(std::time::Duration::from_secs(30))
    );
}
