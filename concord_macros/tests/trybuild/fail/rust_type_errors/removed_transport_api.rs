use concord_core::advanced::{DefaultTransport, ReqwestTransport, Transport};
use concord_core::prelude::{ApiClient, ClientContext};

struct Cx;

impl ClientContext for Cx {
    type Vars = ();
    type AuthVars = ();
    type AuthState = ();

    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTP;
    const DOMAIN: &'static str = "example.test";

    fn init_auth_state(_: &Self::Vars, _: &Self::AuthVars) -> Self::AuthState {}
}

fn main() {
    let _: Option<ApiClient<Cx, DefaultTransport>> = None;
    let _ = ApiClient::<Cx>::with_transport((), (), ReqwestTransport::new());
}
