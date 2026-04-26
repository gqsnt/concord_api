use concord_core::prelude::*;
use concord_macros::api;

api! {
    client UiScopedEndpointRootAliasRemoved {
        base https "example.com"
    }

    scope api {
        GET Ping
        -> Json<()>
        {
        }
    }
}

fn main() {
    let api = ui_scoped_endpoint_root_alias_removed::UiScopedEndpointRootAliasRemoved::new();
    let _ = api.request(ui_scoped_endpoint_root_alias_removed::endpoints::Ping::new());
}
