use super::helpers::*;
use quote::quote;

#[test]
fn generated_explicit_endpoint_api_is_clean_and_matches_facade_target() {
    let out = expanded(quote! {
        client ExplicitApi {
            base "https://example.com"
        }

        GET Get(id: u64, filter?: String, count: u32 = 20)
            as get
            path ["items", id]
            query {
                filter
                count
            }
            -> Json<String>
    });

    assert_contains_all(
        &out,
        &[
            "pub mod endpoints",
            "as Get",
            "#[doc = \"Advanced explicit endpoint request. Prefer facade methods for normal use.\"]",
            "pub fn new ( id : u64 ) -> Self",
            "pub fn filter ( mut self , v : String ) -> Self",
            "pub fn filter_opt ( mut self , v : :: core :: option :: Option < String > ) -> Self",
            "pub fn clear_filter ( mut self ) -> Self",
            "pub fn count ( mut self , v : u32 ) -> Self",
            "pub fn count_opt ( mut self , v : :: core :: option :: Option < u32 > ) -> Self",
            "pub fn clear_count ( mut self ) -> Self",
            "pub fn get (& self , id : u64)",
            "let mut __ep = endpoints :: Get :: new (id)",
            "self . request (__ep)",
        ],
    );
    assert!(
        !out.contains("pubfnid(mutself"),
        "explicit endpoints must not expose setters for required direct args"
    );
}

#[test]
fn generated_request_surface_has_no_profileless_extension_traits() {
    let out = expanded(quote! {
        client RequestSurfaceApi {
            base "https://example.com"
        }

        GET Ping
            as ping
            path ["ping"]
            -> Json<String>
    });

    assert_contains_all(
        &out,
        &["pub fn ping (& self ,) -> :: concord_core :: prelude :: PendingRequest"],
    );
    assert!(
        !out.contains("pub trait PingRequestExt"),
        "endpoints without request setters must not generate empty request-extension traits"
    );
    assert!(
        !out.contains("pub use pending_api :: PingRequestExt"),
        "endpoints without request setters must not reexport empty request-extension traits"
    );
}
