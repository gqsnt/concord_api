//! Writer contracts that are independent of the five tour descriptions.
#![deny(missing_docs)]

#[test]
fn c1_12_local_private_client_and_cfg_preserve_author_scope() {
    use concord::Json;
    type Number = u32;
    concord::api! {
        client Local {
            base "https://local.test";
            read Json;
            #[cfg(any())]
            scope absent(value: MissingType) { GET get() { path "{absent.value}"; } }
            GET number(value: Number) -> Number { query { value; } }
        }
    }
    assert_eq!(
        Local::new().number(3).request().unwrap().url().query(),
        Some("value=3")
    );
}

#[test]
fn c10_3_replacements_evaluate_in_tree_order_and_keep_wire_order() {
    use std::sync::{Arc, Mutex};
    #[derive(Clone)]
    struct Trace(Arc<Mutex<Vec<&'static str>>>);
    impl Trace {
        fn note(&self, text: &'static str) -> String {
            self.0.lock().unwrap().push(text);
            text.to_owned()
        }
    }
    concord::api! {
        client Ordered(trace: Trace) {
            base "https://ordered.test";
            query { "first" = trace.note("discarded"); "second" = trace.note("second"); }
            scope nested { query { override "first" = client.trace.note("first"); }
                GET get() { query { "third" = client.trace.note("third"); } }
            }
        }
    }
    let trace = Trace(Arc::new(Mutex::new(Vec::new())));
    let request = Ordered::new(trace.clone())
        .nested()
        .get()
        .request()
        .unwrap();
    assert_eq!(*trace.0.lock().unwrap(), ["second", "first", "third"]);
    assert_eq!(
        request.url().query(),
        Some("first=first&second=second&third=third")
    );
}

#[test]
fn c1_21_body_function_path_is_not_hidden_by_an_input() {
    use concord::json;
    concord::api! {
        client BodyFunction {
            base "https://body.test";
            POST create(json: String) { body json { json; } }
        }
    }
    let request = BodyFunction::new().create("text").request().unwrap();
    assert_eq!(
        request.body().unwrap().as_bytes().unwrap(),
        br#"{"json":"text"}"#
    );
}

#[test]
fn c2_3_splat_keeps_its_place_between_path_segments() {
    concord::api! {
        client Splat {
            base "https://splat.test";
            GET get(path: String) { path "before/{*path}/after/"; }
        }
    }
    assert_eq!(
        Splat::new().get("one/two").request().unwrap().url().path(),
        "/before/one/two/after/"
    );
}

mod restricted {
    concord::api! {
        pub(in super) client Relative {
            base "https://relative.test";
            scope middle { scope inner { GET get(); } }
        }
    }
}

#[test]
fn c1_12_relative_visibility_is_preserved_at_every_module_depth() {
    assert_eq!(
        restricted::Relative::new()
            .middle()
            .inner()
            .get()
            .request()
            .unwrap()
            .url()
            .host_str(),
        Some("relative.test")
    );
}

#[test]
fn c2_14_literal_body_templates_decode_escaped_braces() {
    use concord::json;
    concord::api! {
        client LiteralBody {
            base "https://body.test";
            POST create() { body json { "text" = "{{literal}}"; } }
        }
    }
    let request = LiteralBody::new().create().request().unwrap();
    assert_eq!(
        request.body().unwrap().as_bytes().unwrap(),
        br#"{"text":"{literal}"}"#
    );
}

#[tokio::test]
async fn c5_12_custom_spelling_of_get_and_head_keeps_method_semantics() {
    concord::api! {
        client CustomMethods {
            base "https://custom.test";
            METHOD "GET" get();
            METHOD "HEAD" head() -> bool { status { 204 => true } }
        }
    }
    let client = CustomMethods::new().with_transport(concord::fake(|request| {
        assert_eq!(request.headers()["if-none-match"], "etag");
        assert!(matches!(request.method().as_str(), "GET" | "HEAD"));
        Ok(concord::reply(304, ""))
    }));
    let get = client.get().send_if_none_match("etag").await.unwrap();
    assert_eq!(get.status().as_u16(), 304);
    assert!(get.value().is_none());
    let head = client.head().send_if_none_match("etag").await.unwrap();
    assert_eq!(head.status().as_u16(), 304);
    assert!(head.value().is_none());
    let _ = client.get().__prepare_if_none_match("etag").unwrap();
    let _ = client.head().__prepare_if_none_match("etag").unwrap();
    let error = client
        .head()
        .read_response(concord::reply(200, "true"))
        .await
        .unwrap_err();
    assert_eq!(error.kind(), concord::error::Kind::Mismatch);
}

concord::api! {
    pub client StrictDocs {
        base "https://strict-docs.test";
        read concord::Json;
        GET value(number: u32, enabled?: bool) -> u32 { query { number; enabled; } }
        scope numbers(group: u32, visible?: bool) {
            path "groups/{group}";
            query { visible; }
            GET list(start?: u32, count?: u32) -> Vec<u32> {
                query { start; count; }
                paginate concord::by_offset_limit(start, count);
            }
        }
    }
}
#[test]
fn c1_16_a_public_client_compiles_with_missing_docs_denied() {
    assert_eq!(
        StrictDocs::new()
            .value(3)
            .enabled(true)
            .request()
            .unwrap()
            .url()
            .query(),
        Some("number=3&enabled=true")
    );
    #[derive(Clone)]
    struct EmptyProblem;
    concord::api! {
        client Empty {
            base "https://empty-docs.test";
            read concord::Json;
            problem EmptyProblem;
        }
    }
    let _ = Empty::new().with_retry(concord::Retry::<EmptyProblem>::none());
}

#[tokio::test]
async fn c2_14_a11_body_templates_reach_the_wire_and_check_every_hole() {
    use concord::{json, Secret};
    concord::api! {
        client Templates {
            base "https://body-template.test";
            POST send(optional?: String, secret?: Secret, numbers?: Vec<u32>) {
                body json { "label" = "prefix:{optional}"; "key" = "Bearer {secret}"; "checked" = "{optional}/{numbers}"; }
            }
        }
    }
    let client = Templates::new().with_transport(concord::fake(|request| {
        assert_eq!(
            request.body().unwrap().as_bytes().unwrap(),
            br#"{"key":"Bearer private"}"#
        );
        Ok(concord::reply(200, ""))
    }));
    client.send().secret("private").await.unwrap();
    let error = client
        .send()
        .secret("private")
        .numbers([1, 2])
        .request()
        .unwrap_err();
    assert_eq!(error.kind(), concord::error::Kind::Build);
    assert!(!format!("{error:?}").contains("private"));
}
