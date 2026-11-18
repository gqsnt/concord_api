//! Provider descriptions and complete guide recipes compile and keep their wire forms.
#![allow(dead_code, clippy::unwrap_used, clippy::indexing_slicing)]

use concord_testkit::Script;

mod key {
    include!("../fixtures/recipes/write_key.rs");
}
mod redirect {
    include!("../fixtures/recipes/redirect.rs");

    #[tokio::test(start_paused = true)]
    async fn c11_12_download_recipe_refuses_undocumented_or_untrusted_redirects() {
        for (status, location, expected) in [
            (200, None, "the archive did not return its documented 302"),
            (302, None, "the download Location is missing"),
            (302, Some("/relative"), "relative URL without a base"),
            (
                302,
                Some("https://internal.test/file"),
                "the download host is not allowed",
            ),
            (
                302,
                Some("https://downloads.example.com/file"),
                "the download host is not allowed",
            ),
            (
                302,
                Some("http://bucket.downloads.example.com/file"),
                "the download host is not allowed",
            ),
            (
                302,
                Some("https://bucket.downloads.example.com:444/file"),
                "the download host is not allowed",
            ),
        ] {
            let script = concord_testkit::Script::new();
            let answer = script.answer(status);
            if let Some(location) = location {
                answer.header("location", location);
            } else {
                drop(answer);
            }
            let api = Archives::new("PRIVATE_API_KEY").with_transport(script.clone());
            // Each case is refused before the second client is made.
            let error = download(&api, 7, Downloads::new).await.unwrap_err();
            assert_eq!(error.to_string(), expected);
            let requests = script.requests();
            assert_eq!(requests.len(), 1);
            assert_eq!(requests[0].url(), "https://api.example.com/archives/7");
            assert_eq!(requests[0].info().number(), 1);
        }
    }

    #[tokio::test(start_paused = true)]
    async fn c11_12_download_recipe_follows_the_location_through_the_transport_it_is_given() {
        let script = concord_testkit::Script::new();
        script.answer(302).header(
            "location",
            "https://eu.downloads.example.com/archives/7?sig=a%2Fb",
        );
        script.answer(200).body("the ").body("archive");
        let archives = Archives::new("PRIVATE_API_KEY").with_transport(script.clone());

        let mut archive = download(&archives, 7, |origin| {
            Downloads::new(origin).with_transport(script.clone())
        })
        .await
        .unwrap();
        let mut bytes = Vec::new();
        while let Some(chunk) = archive.try_next().await.unwrap() {
            bytes.extend_from_slice(&chunk);
        }
        assert_eq!(bytes, b"the archive");

        let sent = script.requests();
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0].url(), "https://api.example.com/archives/7");
        assert_eq!(
            sent[0].header("authorization"),
            Some("Bearer PRIVATE_API_KEY")
        );
        assert_eq!(
            sent[1].url(),
            "https://eu.downloads.example.com/archives/7?sig=a%2Fb"
        );
        assert_eq!(sent[1].header("authorization"), None);
        assert_eq!(sent[1].info().endpoint(), "downloads.fetch");
        assert_eq!(script.unused(), 0);
    }
}
mod empty_body {
    include!("../fixtures/recipes/empty_body.rs");

    #[tokio::test(start_paused = true)]
    async fn c2_12_no_body_an_empty_body_and_an_optional_body_go_out_as_the_guide_says() {
        let script = concord_testkit::Script::new();
        for _ in 0..4 {
            script.answer(204);
        }
        let store = Store::new("PRIVATE_TOKEN").with_transport(script.clone());
        store.archive(7).await.unwrap();
        store.touch(7).await.unwrap();
        store.delete(7).await.unwrap();
        store
            .delete(7)
            .options(DeleteOptions { grace_seconds: 30 })
            .await
            .unwrap();

        let sent = script.requests();
        assert_eq!(sent.len(), 4);
        assert_eq!(sent[0].body(), None);
        assert_eq!(sent[0].header("content-length"), Some("0"));
        for empty in &sent[1..3] {
            assert_eq!(empty.body(), Some(&b""[..]));
            assert_eq!(empty.header("content-type"), None);
            assert_eq!(empty.header("content-length"), None);
        }
        assert_eq!(sent[3].body(), Some(&br#"{"grace_seconds":30}"#[..]));
        assert_eq!(sent[3].header("content-type"), Some("application/json"));
    }
}
mod query {
    include!("../fixtures/recipes/query.rs");
}
mod gzip {
    include!("../fixtures/recipes/gzip.rs");
}

#[test]
fn c1_20_writer_recipe_blocks_match_the_guide_word_for_word() {
    for (page, code) in [
        (
            include_str!("../../../docs/guide/retries.md"),
            include_str!("../fixtures/recipes/write_key.rs"),
        ),
        (
            include_str!("../../../docs/guide/extending.md"),
            include_str!("../fixtures/recipes/redirect.rs"),
        ),
        (
            include_str!("../../../docs/guide/describing-an-api.md"),
            include_str!("../fixtures/recipes/query.rs"),
        ),
        (
            include_str!("../../../docs/guide/bodies.md"),
            include_str!("../fixtures/recipes/gzip.rs"),
        ),
        (
            include_str!("../../../docs/guide/bodies.md"),
            include_str!("../fixtures/recipes/empty_body.rs"),
        ),
    ] {
        assert!(page.contains(&format!("```rust\n{}\n```", code.trim())));
    }
}

#[tokio::test(start_paused = true)]
async fn c12_1_stripe_uses_the_shipped_form_with_expansion_metadata_and_unit_variants() {
    use real_apis::stripe::{CollectionMethod, CustomerId, Stripe};
    let script = Script::new();
    script.answer(204);
    script.answer(204);
    let stripe = Stripe::new("PRIVATE_API_KEY").with_transport(script.clone());
    stripe
        .customers()
        .create("order-key")
        .email("ada@example.com")
        .metadata(std::collections::BTreeMap::from([(
            "order".into(),
            "7781".into(),
        )]))
        .expand(["customer"])
        .send_raw()
        .await
        .unwrap();
    stripe
        .invoices()
        .create(CustomerId::from("cus_1"), "invoice-key")
        .collection_method(CollectionMethod::SendInvoice)
        .send_raw()
        .await
        .unwrap();
    let requests = script.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests
            .iter()
            .map(|request| request.info().number())
            .collect::<Vec<_>>(),
        [1, 1]
    );
    assert_eq!(
        requests[0].body().unwrap(),
        b"expand%5B0%5D=customer&email=ada%40example.com&metadata%5Border%5D=7781"
    );
    assert_eq!(
        requests[1].body().unwrap(),
        b"customer=cus_1&collection_method=send_invoice"
    );
    assert_eq!(requests[0].header("idempotency-key"), Some("order-key"));
    assert_eq!(requests[1].header("idempotency-key"), Some("invoice-key"));
}

#[tokio::test(start_paused = true)]
async fn c12_4_elasticsearch_uses_bounded_json_lines_without_changing_action_records() {
    use real_apis::elasticsearch::Elasticsearch;
    let script = Script::new();
    script.answer(204);
    let api = Elasticsearch::new(
        "https://search.test".parse::<concord::Base>().unwrap(),
        "PRIVATE_API_KEY",
    )
    .with_transport(script.clone());
    api.bulk(vec![
        serde_json::json!({"index":{"_id":"42"}}),
        serde_json::json!({"title":"Dune"}),
    ])
    .send_raw()
    .await
    .unwrap();
    let requests = script.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].info().number(), 1);
    assert_eq!(
        requests[0].body().unwrap(),
        b"{\"index\":{\"_id\":\"42\"}}\n{\"title\":\"Dune\"}\n"
    );
    assert_eq!(
        requests[0].header("content-type"),
        Some("application/x-ndjson")
    );
    let too_large = api
        .bulk(vec![serde_json::json!("x".repeat(10 * concord::MIB))])
        .send_raw()
        .await
        .unwrap_err();
    assert_eq!(too_large.kind(), concord::error::Kind::Build);
    assert_eq!(too_large.tries(), 0);
    assert!(matches!(
        concord::JsonLinesError::find(&too_large),
        Some(concord::JsonLinesError::TooLarge)
    ));
    assert_eq!(script.requests().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn c4_1_uuid_is_stable_across_attempts_and_fresh_for_a_second_call() {
    let script = Script::new();
    script.answer(503);
    script.answer(204);
    script.answer(204);
    let api = key::Payments::new()
        .with_transport(script.clone())
        .with_retry(concord::Retry::standard());
    api.create(100).await.unwrap();
    api.create(100).await.unwrap();
    let requests = script.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(
        requests
            .iter()
            .map(|request| request.info().number())
            .collect::<Vec<_>>(),
        [1, 2, 1]
    );
    let keys: Vec<_> = requests
        .iter()
        .map(|request| request.header("idempotency-key").unwrap())
        .collect();
    assert_eq!(keys[0], keys[1]);
    assert_ne!(keys[1], keys[2]);
    for key in keys {
        let uuid = uuid::Uuid::parse_str(key).unwrap();
        assert_eq!(uuid.get_version_num(), 4);
        assert_eq!(uuid.to_string(), key);
    }
}

#[tokio::test(start_paused = true)]
async fn c8_6_redirect_uses_a_second_credential_free_description_after_an_allowlist() {
    let script = Script::new();
    let location = "https://bucket.downloads.example.com/file?sig=%2f+&x=1&x=2";
    script.answer(302).header("location", location);
    script.answer(200).body("file");
    let api = redirect::Archives::new("PRIVATE_API_KEY").with_transport(script.clone());
    let response = api.download(7).send_raw().await.unwrap();
    assert_eq!(script.requests().len(), 1);
    let location = response
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let url = concord::reqwest::Url::parse(&location).unwrap();
    assert_eq!(url.scheme(), "https");
    assert_eq!(url.port_or_known_default(), Some(443));
    assert!(url.host_str().unwrap().ends_with(".downloads.example.com"));
    let origin: concord::Base = url.origin().ascii_serialization().parse().unwrap();
    drop(response);
    let downloads = redirect::Downloads::new(origin).with_transport(script.clone());
    let mut body = concord::transfer::at(downloads.fetch(), &location)
        .unwrap()
        .await
        .unwrap();
    assert_eq!(body.try_next().await.unwrap().unwrap(), "file");
    assert!(body.try_next().await.unwrap().is_none());
    let requests = script.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests
            .iter()
            .map(|request| request.info().number())
            .collect::<Vec<_>>(),
        [1, 1]
    );
    assert_eq!(
        requests[0].header("authorization"),
        Some("Bearer PRIVATE_API_KEY")
    );
    assert_eq!(requests[1].header("authorization"), None);
    assert_eq!(requests[1].url(), location);
    let error = concord::transfer::at(downloads.fetch(), "https://internal.test/file")
        .unwrap()
        .await
        .unwrap_err();
    assert_eq!(error.kind(), concord::error::Kind::Build);
    assert_eq!(error.tries(), 0);
    assert_eq!(script.requests().len(), 2);
}
