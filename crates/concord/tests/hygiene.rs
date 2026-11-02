//! `api!` forwarded through `macro_rules!` (ADV-3), and generated entry methods with many required
//! values (ADV-11: `clippy::too_many_arguments` is allowed on them; checked by the clippy gate).

use concord::reqwest;

macro_rules! make_api {
    ($($body:tt)*) => {
        concord::api! {
            /// Forwarded declaration.
            pub client Hyg {
                base "http://h.invalid";
                $($body)*
            }
        }
    };
}

make_api!(GET e(query q?: String, query r: String, id: u32) { path "x/{id}"; headers { "x-r" = "{r}"; } });

concord::api! {
    /// Many required values.
    pub client Wide(query a: u8, query b: u8, query c: u8, query d: u8, query e: u8, query f: u8, query g: u8, query h: u8) {
        base "http://w.invalid";
        GET one(query i: u8, query j: u8, query k: u8, query l: u8, query m: u8, query n: u8, query o: u8, query p: u8);
    }
}

#[test]
fn forwarded_declaration_builds_requests() {
    let p = Hyg::new(reqwest::Client::new())
        .e("rv", 3)
        .set_q("qv")
        .prepare()
        .unwrap();
    assert_eq!(p.request().url().as_str(), "http://h.invalid/x/3?q=qv&r=rv");
    assert_eq!(p.request().headers()["x-r"], "rv");
}

#[test]
fn many_required_values() {
    let p = Wide::new(
        reqwest::Client::new(),
        WideConfig {
            a: 1,
            b: 2,
            c: 3,
            d: 4,
            e: 5,
            f: 6,
            g: 7,
            h: 8,
        },
    )
    .one(9, 10, 11, 12, 13, 14, 15, 16)
    .prepare()
    .unwrap();
    assert_eq!(
        p.request().url().query(),
        Some("a=1&b=2&c=3&d=4&e=5&f=6&g=7&h=8&i=9&j=10&k=11&l=12&m=13&n=14&o=15&p=16")
    );
}

/// A raw-keyword client name keeps its raw spelling (milestone review F-02): `r#type` generated
/// `type`, which is not an identifier.
#[allow(non_camel_case_types)]
mod raw_root {
    concord::api! {
        /// A client named with a keyword.
        pub client r#type {
            base "http://raw.example.test";
            /// The only endpoint.
            GET ping() { path "ping"; }
        }
    }

    #[test]
    fn raw_keyword_client_name() {
        let api = r#type::new(concord::reqwest::Client::new());
        assert_eq!(
            format!("{api:?}"),
            r#"type { base: "http://raw.example.test/" }"#
        );
        let p = api.ping().prepare().unwrap();
        assert_eq!(p.request().url().path(), "/ping");
    }
}

/// Generated code names its prelude items by path: it expands in a `no_implicit_prelude` module
/// (the declaration's own types are written as paths there too).
#[no_implicit_prelude]
mod no_prelude {
    ::concord::api! {
        /// Declared without the prelude.
        pub(crate) client Bare(query lang?: ::std::string::String = "en") {
            base "http://bare.invalid";
            GET item(id: u32, header("x-n") n?: u32) -> ::std::string::String {
                path "items/{id}";
                response text;
            }
        }
    }
}

#[test]
fn expands_without_the_prelude() {
    let p = no_prelude::Bare::new(reqwest::Client::new())
        .item(7)
        .set_n(2u32)
        .prepare()
        .unwrap();
    assert_eq!(
        p.request().url().as_str(),
        "http://bare.invalid/items/7?lang=en"
    );
    assert_eq!(p.request().headers()["x-n"], "2");
}
