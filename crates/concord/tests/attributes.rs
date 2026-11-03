//! D14: `#[cfg]` on scopes and endpoints removes the whole subtree (no dangling methods, types or
//! references); `#[deprecated]` applies to the navigation method. Compiles under
//! `deny(warnings)`.
#![deny(warnings)]

use concord::reqwest;

concord::api! {
    /// Attribute fixture.
    pub client Attrs {
        base "http://attrs.invalid";

        /// Always present.
        GET kept() { path "kept"; }

        /// Disabled: neither its output type nor its format type exists (A09: a disabled endpoint
        /// resolves no format).
        #[cfg(any())]
        GET gone() -> DoesNotExist {
            response NoSuchFormat;
            path "gone";
        }

        /// Disabled scope with a disabled-only value type and children.
        #[cfg(any())]
        scope hidden(id: AlsoMissing) {
            path "hidden/{id}";
            GET inner() { path "inner"; }
        }

        /// A default whose only consumer is disabled, and one with no consumer: neither format
        /// type exists, and neither is resolved (A09).
        scope defaults {
            response NoSuchFormat;
            request AlsoNoSuchFormat;
            #[cfg(any())]
            GET only_user() -> DoesNotExist { path "only"; }
            GET ping() { path "ping"; }
        }

        /// Enabled through `cfg(all())`.
        #[cfg(all())]
        scope shown(id: u32) {
            path "shown/{id}";
            /// Deprecated endpoint.
            #[deprecated(note = "use `kept`")]
            GET old() { path "old"; }
        }
    }
}

#[test]
fn cfg_and_deprecated() {
    let api = Attrs::new(reqwest::Client::new());
    let p = api.kept().prepare().unwrap();
    assert_eq!(p.request().url().path(), "/kept");
    #[allow(deprecated)]
    let p = api.shown(7).old().prepare().unwrap();
    assert_eq!(p.request().url().path(), "/shown/7/old");
}
