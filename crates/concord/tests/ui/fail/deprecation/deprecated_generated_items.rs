// V3-O01 / §3.5 / DOC-01 (§19.2): deprecation is emitted where calling the
// deprecated operation actually triggers the warning. Under
// `#![deny(deprecated)]` each call site of a deprecated generated item is an
// error: an endpoint constructor (bound and unbound), every value control of
// a deprecated no-op value (§4.6), a scope navigation (bound and
// unbound) and a client type. The declarations themselves, and the
// non-deprecated items beside them, produce no deprecation diagnostic: the
// generated support code never reports its own deprecated items.
#![deny(deprecated)]

use concord::{Binary, Bytes, api};

api! {
    /// Legacy provider client.
    client LegacyApi {
        base "https://legacy.example.test/";

        values {
            /// Retired filter, no longer sent.
            #[deprecated(note = "the provider ignores this filter")]
            legacy_filter?: String;
        }

        /// Returns the complete export.
        #[deprecated(since = "3.0.0", note = "use the paged feed instead")]
        GET export() -> Bytes via Binary {
            path "export";
        }

        /// Current paged feed.
        GET feed(
            query cursor?: String,
            #[deprecated(note = "page size is fixed by the provider")] page_size?: u32,
        ) -> Bytes via Binary {
            path "feed";
        }

        /// Retired regional routing.
        #[deprecated(note = "regions are resolved by the provider")]
        scope regional(region: String) {
            path "regions/{region}";

            GET status() -> Bytes via Binary {
                path "status";
            }
        }
    }
}

api! {
    /// Retired client.
    #[deprecated(note = "use LegacyApi")]
    client RetiredApi {
        base "https://retired.example.test/";

        GET ping() -> Bytes via Binary;
    }
}

fn main() {
    // Non-deprecated items: no diagnostic.
    let client = LegacyApi::new().expect("client");
    let _ = client.feed().set_cursor("next");

    // Deprecated endpoint: bound and unbound constructors.
    let _ = client.export();
    let _ = LegacyApi::unbound().export();

    // Deprecated no-op value bridges: root builder and request controls.
    let _ = LegacyApi::builder().set_legacy_filter("x");
    let _ = LegacyApi::builder().omit_legacy_filter();
    let _ = client.feed().replace_page_size(Some(10));

    // Deprecated scope: bound and unbound navigation.
    let _ = client.regional("eu");
    let _ = LegacyApi::unbound().regional("eu");

    // Deprecated client type.
    let _ = RetiredApi::new();
}
