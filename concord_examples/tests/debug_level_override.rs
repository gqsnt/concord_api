use concord_core::prelude::*;
use concord_macros::api;

api! {
    client Client {
        scheme: https,
        host: "example.com",
        params { }
        headers { }
    }

    path "ping" {
        GET Ping "" -> TextEncoding<String>;
    }
}

#[test]
fn debug_level_inherit_and_override() {
    // client-side default
    let vars = client::ClientVars::new();
    let client = ApiClient::<client::ClientCx>::new(vars).with_debug_level(DebugLevel::V);
    assert_eq!(client.debug_level(), DebugLevel::V);

    // endpoint: inherit
    let ep_inherit = client::endpoints::Ping::new();
    let eff = ep_inherit.debug_level().unwrap_or(client.debug_level());
    assert_eq!(eff, DebugLevel::V);

    // endpoint: override to VV
    let ep_vv = client::endpoints::Ping::new().with_debug_level(DebugLevel::VV);
    assert_eq!(ep_vv.debug_level(), Some(DebugLevel::VV));
    let eff = ep_vv.debug_level().unwrap_or(client.debug_level());
    assert_eq!(eff, DebugLevel::VV);

    // endpoint: override to None (disable)
    let client2 = client.clone().with_debug_level(DebugLevel::VV);
    let ep_none = client::endpoints::Ping::new().with_debug_level(DebugLevel::None);
    let eff = ep_none.debug_level().unwrap_or(client2.debug_level());
    assert_eq!(eff, DebugLevel::None);

    // generated wrapper also compiles with with_debug_level()
    let _wrapper = client::Client::new().with_debug_level(DebugLevel::V);
}
