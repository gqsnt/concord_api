//! `concord` depended on as `concord4` (ADV-4): expansions refer to the crate through `$crate`,
//! so the format words (`text`, `bytes`) need no import, and a format type path resolves like any
//! type (`concord4::Text`). Native header values are named through a renamed `reqwest` and an
//! alias: Concord recognizes types by trait, never by spelling.

/// An alias of the native header value, through the renamed `reqwest` dependency.
pub type Native = reqwest013::header::HeaderValue;

/// A static native value.
pub static ACCEPT: Native = Native::from_static("text/plain");

concord4::api! {
    /// A client declared through the renamed crate.
    pub client Renamed(query("v") version: u32 = 4) {
        base "http://renamed.invalid";
        GET ping() -> text String { path "ping"; }
        PUT upload(bytes data: Vec<u8>) -> concord4::Binary Vec<u8> { path "upload"; }
    }
}

concord4::api! {
    /// Native header values named through the renamed `reqwest`, an alias and a qualified path.
    pub client Headers(
        header("accept") accept: Native = &ACCEPT,
        header("x-tag") each tags?: Vec<concord4::reqwest::header::HeaderValue>,
    ) {
        base "http://renamed.invalid";
        request text;
        response text;
        GET ping() -> String { path "ping"; }
        POST note(body note: String) { path "note"; }
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
    fn formats_through_the_renamed_crate() {
        let api = super::Renamed::new(concord4::reqwest::Client::new());
        let p = api.upload(vec![1, 2]).prepare().unwrap();
        assert_eq!(
            p.request().headers()["content-type"],
            "application/octet-stream"
        );
        let api = super::Headers::new(concord4::reqwest::Client::new());
        let p = api.note("hi").prepare().unwrap();
        assert_eq!(
            p.request().headers()["content-type"],
            "text/plain; charset=utf-8"
        );
        assert_eq!(p.request().body().unwrap().as_bytes(), Some(&b"hi"[..]));
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
