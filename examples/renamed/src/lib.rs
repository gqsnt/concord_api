//! `concord` depended on as `concord4` (ADV-4): expansions refer to the crate through `$crate`.
//! Native header values are named through a renamed `reqwest` and an alias: Concord recognizes
//! types by trait, never by spelling.

/// An alias of the native header value, through the renamed `reqwest` dependency.
pub type Native = reqwest013::header::HeaderValue;

/// A static native value.
pub static ACCEPT: Native = Native::from_static("text/plain");

concord4::api! {
    /// A client declared through the renamed crate.
    pub client Renamed(query("v") version: u32 = 4) {
        base "http://renamed.invalid";
        GET ping() -> String via concord4::Text { path "ping"; }
    }
}

concord4::api! {
    /// Native header values named through the renamed `reqwest`, an alias and a qualified path.
    pub client Headers(
        header("accept") accept: Native = &ACCEPT,
        header("x-tag") each tags?: Vec<concord4::reqwest::header::HeaderValue>,
    ) {
        base "http://renamed.invalid";
        GET ping() -> String via concord4::Text { path "ping"; }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn builds_a_request() {
        let p = super::Renamed::new(concord4::reqwest::Client::new())
            .ping()
            .prepare()
            .unwrap();
        assert_eq!(
            p.request().url().as_str(),
            "http://renamed.invalid/ping?v=4"
        );
    }

    #[test]
    fn native_headers_through_renamed_dependencies() {
        let mut secret = super::Native::from_bytes(b"caf\xe9").unwrap();
        secret.set_sensitive(true);
        let api = super::Headers::new(concord4::reqwest::Client::new())
            .set_tags(vec![secret, super::ACCEPT.clone()]);
        let p = api.ping().prepare().unwrap();
        let headers = p.request().headers();
        assert_eq!(headers["accept"], "text/plain");
        let tags: Vec<_> = headers.get_all("x-tag").iter().collect();
        assert_eq!(tags[0].as_bytes(), b"caf\xe9");
        assert!(tags[0].is_sensitive());
        assert!(!tags[1].is_sensitive());
        assert_eq!(
            format!("{api:?}"),
            r#"Headers { base: "http://renamed.invalid/", accept: "text/plain", tags: Some([<redacted>, "text/plain"]) }"#
        );
    }
}
