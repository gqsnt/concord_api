use concord_core::advanced::Caps;
use concord_core::prelude::{ApiClient, ClientContext, DebugLevel};

#[derive(Clone)]
struct TestCx;

impl ClientContext for TestCx {
    type Vars = ();
    type AuthVars = ();
    type AuthState = ();

    const SCHEME: http::uri::Scheme = http::uri::Scheme::HTTPS;
    const DOMAIN: &'static str = "example.com";

    fn init_auth_state(_: &Self::Vars, _: &Self::AuthVars) -> Self::AuthState {}
}

#[test]
fn configure_updates_debug_level_and_pagination_caps() {
    let mut api = ApiClient::<TestCx>::new((), ());

    api.configure(|cfg| {
        cfg.debug_level(DebugLevel::VV)
            .pagination_caps(Caps::default().max_pages(3).max_items(12));
    });

    assert_eq!(api.debug_level(), DebugLevel::VV);
    assert_eq!(api.pagination_caps().max_pages, 3);
    assert_eq!(api.pagination_caps().max_items, 12);
}
