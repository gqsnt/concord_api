//! Native header names as keys (`header(AUTHORIZATION)`, `X_RIOT_TOKEN = v`): identity, composition
//! and classification follow the resolved `HeaderName`, never the spelling of the path.

use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};

use concord::reqwest::header::AUTHORIZATION as AUTH;
use concord::reqwest::header::{self, ACCEPT, AUTHORIZATION, CONTENT_TYPE, HOST, HeaderName};
use concord::{ErrorKind, reqwest};

/// A vendor header, defined the native way.
pub const X_RIOT_TOKEN: HeaderName = HeaderName::from_static("x-riot-token");
/// The same header as a `static`.
pub static X_RIOT_TOKEN_STATIC: HeaderName = HeaderName::from_static("x-riot-token");

/// A constant whose identifier says `AUTHORIZATION` but whose name is `x-other` (H04).
mod shadow {
    use concord::reqwest::header::HeaderName;
    pub const AUTHORIZATION: HeaderName = HeaderName::from_static("x-other");
}

/// Re-exported standard constants (H03).
mod keys {
    pub use concord::reqwest::header::ACCEPT_LANGUAGE as LANG;
}

static FORMATTED: AtomicUsize = AtomicUsize::new(0);

/// Counts its `Display` calls (H15: a removed or replaced value is never formatted).
#[derive(Clone)]
pub struct Counted(pub &'static str);

impl fmt::Display for Counted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        FORMATTED.fetch_add(1, Ordering::SeqCst);
        f.write_str(self.0)
    }
}

concord::api! {
    /// Native keys in direct placements and blocks.
    pub client Keys(
        token: String,
        #[sensitive] header(X_RIOT_TOKEN) riot: String,
        header(keys::LANG) lang?: String = "en",
    ) {
        base "http://keys.invalid";
        headers {
            AUTHORIZATION = "Bearer {token}";
            header::ACCEPT = "application/json";
        }

        /// Everything inherited.
        GET plain();
        /// H05: a literal spelling replaces the native one.
        GET literal_override(fresh: String) { headers { override "Authorization" = "Bearer {fresh}"; } }
        /// H05: an alias of the same constant removes it.
        GET alias_remove() { headers { remove AUTH; } }
        /// H05: an absolute path appends to it.
        GET absolute_append() { headers { append ::concord::reqwest::header::ACCEPT = "text/plain"; } }
        /// H03: the `static` names the same header as the constant.
        GET static_override(other: String) { headers { override X_RIOT_TOKEN_STATIC = other; } }
        /// `else inherit` across spellings.
        GET kept(maybe?: String) { headers { override "authorization" = maybe else inherit; } }
        /// H05: a literal spelling of a header set natively is "already set" (deferred).
        GET duplicate(again: String) { headers { "authorization" = again; } }
        /// H04: the shadowing constant is `x-other`; it removes nothing the chain sets (deferred).
        GET shadow_remove() { headers { remove shadow::AUTHORIZATION; } }
        /// H04: introduced, it is simply `x-other`.
        GET shadow_set(other: String) { headers { shadow::AUTHORIZATION = other; } }
    }
}

concord::api! {
    /// H15: removed or replaced native contributions are never formatted.
    pub client Lazy(first: Counted) {
        base "http://lazy.invalid";
        headers { X_RIOT_TOKEN = "{first}"; }
        GET removed() { headers { remove "x-riot-token"; } }
        GET replaced(second: Counted) { headers { override X_RIOT_TOKEN_STATIC = "{second}"; } }
        GET kept();
    }
}

concord::api! {
    /// H14: classification by resolved name, whatever the spelling; positive controls on the wire.
    pub client Classified(
        header(AUTHORIZATION) unmarked: String,
        header(shadow::AUTHORIZATION) other: String,
        header(ACCEPT) accept: String = "application/json",
    ) {
        base "http://classified.invalid";
        GET get();
        /// A value under a native key that is also written as text is withheld everywhere.
        GET echo() { path "echo/{client.unmarked}"; }
    }
}

concord::api! {
    /// Stickiness across spellings: a marked native contribution makes a later literal one sensitive.
    pub client Sticky(#[sensitive] header(X_RIOT_TOKEN) key: String) {
        base "http://sticky.invalid";
        GET replaced(public: String) { headers { override "x-riot-token" = public; } }
        GET unrelated(public: String) { headers { "x-public" = public; } }
    }
}

concord::api! {
    /// Reserved names and `content-type` through native constants.
    pub client Policy {
        base "http://policy.invalid";
        /// Declaring a transport-managed header is an error, present or not.
        GET host_set(header(HOST) h?: String);
        POST typed(header(CONTENT_TYPE) ct: String, text b: String);
        POST conflict(header(CONTENT_TYPE) ct: String, text("text/markdown") b: String);
    }
}

fn http() -> reqwest::Client {
    reqwest::Client::builder().no_proxy().build().unwrap()
}

fn keys() -> Keys {
    Keys::new(
        http(),
        KeysConfig {
            token: "tok".into(),
            riot: "RGAPI-fixture".into(),
        },
    )
}

fn values(p: &concord::Prepared<()>, name: &str) -> Vec<Vec<u8>> {
    p.request()
        .headers()
        .get_all(name)
        .iter()
        .map(|v| v.as_bytes().to_vec())
        .collect()
}

#[test]
fn h01_h03_native_keys_send_their_names() {
    let p = keys().plain().prepare().unwrap();
    assert_eq!(values(&p, "authorization"), [b"Bearer tok".to_vec()]);
    assert_eq!(values(&p, "accept"), [b"application/json".to_vec()]);
    assert_eq!(values(&p, "x-riot-token"), [b"RGAPI-fixture".to_vec()]);
    assert_eq!(values(&p, "accept-language"), [b"en".to_vec()]);
}

#[test]
fn h05_composition_follows_identity_not_spelling() {
    let p = keys().literal_override("new").prepare().unwrap();
    assert_eq!(values(&p, "authorization"), [b"Bearer new".to_vec()]);
    let p = keys().alias_remove().prepare().unwrap();
    assert!(values(&p, "authorization").is_empty());
    let p = keys().absolute_append().prepare().unwrap();
    assert_eq!(
        values(&p, "accept"),
        [b"application/json".to_vec(), b"text/plain".to_vec()]
    );
    let p = keys().static_override("other").prepare().unwrap();
    assert_eq!(values(&p, "x-riot-token"), [b"other".to_vec()]);
    let p = keys().kept().prepare().unwrap();
    assert_eq!(values(&p, "authorization"), [b"Bearer tok".to_vec()]);
    let p = keys().kept().set_maybe("Bearer x").prepare().unwrap();
    assert_eq!(values(&p, "authorization"), [b"Bearer x".to_vec()]);
}

#[test]
fn identity_errors_are_deferred_build_errors() {
    let error = keys().duplicate("x").prepare().unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Build);
    let text = error.to_string();
    assert!(
        text.contains("`authorization` on `Keys.duplicate`"),
        "{text}"
    );
    assert!(text.contains("already sets"), "{text}");
    assert!(!text.contains("tok"), "{text}");

    // H04: the shadowing constant is `x-other`, which nothing sets.
    let error = keys().shadow_remove().prepare().unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Build);
    assert!(
        error.to_string().contains("no enclosing owner sets"),
        "{error}"
    );
    let p = keys().shadow_set("o").prepare().unwrap();
    assert_eq!(values(&p, "x-other"), [b"o".to_vec()]);
    assert_eq!(values(&p, "authorization"), [b"Bearer tok".to_vec()]);
}

#[test]
fn h15_suppressed_values_are_never_formatted() {
    let lazy = Lazy::new(
        http(),
        LazyConfig {
            first: Counted("a"),
        },
    );
    let before = FORMATTED.load(Ordering::SeqCst);
    let p = lazy.removed().prepare().unwrap();
    assert!(values(&p, "x-riot-token").is_empty());
    assert_eq!(FORMATTED.load(Ordering::SeqCst), before);
    let p = lazy.replaced(Counted("b")).prepare().unwrap();
    assert_eq!(values(&p, "x-riot-token"), [b"b".to_vec()]);
    assert_eq!(FORMATTED.load(Ordering::SeqCst), before + 1);
    let p = lazy.kept().prepare().unwrap();
    assert_eq!(values(&p, "x-riot-token"), [b"a".to_vec()]);
    assert_eq!(FORMATTED.load(Ordering::SeqCst), before + 2);
}

#[test]
fn h14_classification_by_resolved_name() {
    let api = Classified::new(
        http(),
        ClassifiedConfig {
            unmarked: "S3CR3T".into(),
            other: "visible".into(),
        },
    );
    let shown = format!("{api:?}");
    assert!(!shown.contains("S3CR3T"), "{shown}");
    assert!(shown.contains("other: \"visible\""), "{shown}");
    assert!(shown.contains("accept: \"application/json\""), "{shown}");
    let p = api.get().prepare().unwrap();
    let headers = p.request().headers();
    // Positive controls: the bytes are sent; the flag follows the resolved name.
    assert_eq!(headers["authorization"].as_bytes(), b"S3CR3T");
    assert!(headers["authorization"].is_sensitive());
    assert!(!headers["x-other"].is_sensitive());
    assert!(!headers["accept"].is_sensitive());
    let shown = format!("{p:?}");
    assert!(
        !shown.contains("S3CR3T") && shown.contains("visible"),
        "{shown}"
    );
    // Written as text too: withheld from the URL as well.
    let p = api.echo().prepare().unwrap();
    assert_eq!(p.request().url().path(), "/echo/S3CR3T");
    let shown = format!("{p:?}");
    assert!(!shown.contains("S3CR3T"), "{shown}");
}

#[test]
fn marked_native_contribution_is_sticky_across_spellings() {
    let api = Sticky::new(http(), StickyConfig { key: "k".into() });
    let p = api.replaced("PUBVAL").prepare().unwrap();
    let value = &p.request().headers()["x-riot-token"];
    assert_eq!(value.as_bytes(), b"PUBVAL");
    assert!(value.is_sensitive());
    assert!(!format!("{p:?}").contains("PUBVAL"), "{p:?}");
    let request = api.replaced("PUBVAL");
    assert!(!format!("{request:?}").contains("PUBVAL"), "{request:?}");
    // An unrelated header stays visible.
    let p = api.unrelated("PUBVAL").prepare().unwrap();
    assert!(!p.request().headers()["x-public"].is_sensitive());
    let request = api.unrelated("PUBVAL");
    assert!(format!("{request:?}").contains("\"PUBVAL\""), "{request:?}");
}

#[test]
fn reserved_and_content_type_through_native_constants() {
    let api = Policy::new(http());
    for request in [api.host_set(), api.host_set().set_h("h")] {
        let error = request.prepare().unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Build);
        assert!(error.to_string().contains("reqwest/hyper"), "{error}");
    }
    // A native `content-type` wins over the body format's media type.
    let p = api.typed("application/x-custom", "body").prepare().unwrap();
    assert_eq!(
        p.request()
            .headers()
            .get_all(CONTENT_TYPE)
            .iter()
            .collect::<Vec<_>>(),
        ["application/x-custom"]
    );
    // ... and conflicts with a declared media type.
    let error = api.conflict("a/b", "body").prepare().unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Build);
    assert!(error.to_string().contains("media type"), "{error}");
}
