//! The S3 adapter and HMAC guide recipe run through the public facade.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use concord::sign::{Credentials, SignError};
use concord::Base;
use concord_testkit::Script;
use real_apis::credential_signing::Vault;
// Compile the page's sources in this test crate, where cfg(test) selects wire.rs.
#[path = "../src/s3/mod.rs"]
mod s3;
use s3::ObjectStore;

#[tokio::test(start_paused = true)]
async fn s3_create_bucket_omits_body_until_configuration_is_set() {
    let origin: Base = "https://bucket.s3.amazonaws.com".parse().unwrap();
    let script = Script::new();
    script.answer(200);
    script.answer(200);
    let bucket = ObjectStore::new(
        "us-east-1",
        Credentials::new("ACCESS", "secret"),
        vec![origin.clone()],
    )
    .with_transport(script.clone())
    .bucket(origin);
    bucket.create().await.unwrap();
    bucket
        .create()
        .configuration(s3::CreateBucketConfiguration {})
        .await
        .unwrap();

    let requests = script.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].body(), None);
    assert_eq!(requests[0].header("content-type"), None);
    assert_eq!(
        requests[1].body(),
        Some(&b"<CreateBucketConfiguration/>"[..])
    );
    assert_eq!(requests[1].header("content-type"), Some("application/xml"));
}

#[tokio::test(start_paused = true)]
async fn c12_9_s3_signs_two_bucket_hosts_and_the_global_endpoint_for_their_regions() {
    let first: Base = "https://first.s3.eu-west-1.amazonaws.com".parse().unwrap();
    let second: Base = "https://s3.eu-west-1.amazonaws.com/second".parse().unwrap();
    let script = Script::new();
    for _ in 0..3 {
        script.answer(200);
    }
    let store = ObjectStore::new(
        "eu-west-1",
        Credentials::new("ACCESS", "secret"),
        vec![first.clone(), second.clone()],
    )
    .with_transport(script.clone());
    store.bucket(first).head().await.unwrap();
    store.bucket(second).head().await.unwrap();
    store.list_buckets().send_raw().await.unwrap();
    let requests = script.requests();
    assert_eq!(requests.len(), 3);
    assert_eq!(
        requests
            .iter()
            .map(|r| r.info().number())
            .collect::<Vec<_>>(),
        [1, 1, 1]
    );
    for request in &requests[..2] {
        assert!(request
            .header("authorization")
            .unwrap()
            .contains("/eu-west-1/s3/aws4_request"));
    }
    assert!(requests[2]
        .header("authorization")
        .unwrap()
        .contains("/us-east-1/s3/aws4_request"));
    assert_eq!(
        requests[0].header("host"),
        Some("first.s3.eu-west-1.amazonaws.com")
    );
    assert_eq!(
        requests[1].header("host"),
        Some("s3.eu-west-1.amazonaws.com")
    );
    assert_eq!(requests[2].header("host"), Some("s3.amazonaws.com"));
}

#[tokio::test(start_paused = true)]
async fn c12_10_s3_refuses_an_unconfigured_bucket_before_using_invalid_credentials() {
    let script = Script::new();
    let store = ObjectStore::new("eu-west-1", Credentials::new("", ""), Vec::<Base>::new())
        .with_transport(script.clone());
    let error = store
        .bucket("https://foreign.test".parse::<Base>().unwrap())
        .head()
        .await
        .unwrap_err();
    assert!(matches!(
        SignError::find(&error),
        Some(SignError::WrongOrigin)
    ));
    assert_eq!(script.requests().len(), 0);
}

#[tokio::test(start_paused = true)]
async fn c12_9_s3_signs_the_xml_bulk_delete_body_and_checksum() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use md5::{Digest, Md5};
    let origin: Base = "https://bucket.s3.eu-west-1.amazonaws.com".parse().unwrap();
    let script = Script::new();
    script.answer(200).body(concat!(
        "<DeleteResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">",
        "<Deleted><Key>reports/old.txt</Key></Deleted></DeleteResult>"
    ));
    let store = ObjectStore::new(
        "eu-west-1",
        Credentials::new("ACCESS", "secret"),
        vec![origin.clone()],
    )
    .with_transport(script.clone());
    let result = store
        .bucket(origin)
        .delete_objects(s3::DeleteBatch::new(["reports/old.txt".into()]))
        .await
        .unwrap();
    assert_eq!(result.deleted[0].key, "reports/old.txt");
    let requests = script.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].info().number(), 1);
    let expected_xml = concat!(
        "<Delete xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">",
        "<Object><Key>reports/old.txt</Key></Object><Quiet>false</Quiet></Delete>"
    );
    assert_eq!(requests[0].body(), Some(expected_xml.as_bytes()));
    assert_eq!(requests[0].header("content-type"), Some("application/xml"));
    assert!(include_str!("../../../docs/examples/s3.md").contains(expected_xml));
    let digest = STANDARD.encode(Md5::digest(expected_xml.as_bytes()));
    assert_eq!(requests[0].header("content-md5"), Some(digest.as_str()));
    verify_s3(
        &requests[0],
        "/",
        "delete=",
        "eu-west-1",
        "content-md5;content-type;host;x-amz-content-sha256;x-amz-date",
    );
}

fn verify_s3(
    request: &concord_testkit::script::Request,
    path: &str,
    query: &str,
    region: &str,
    names: &str,
) {
    use hmac::{Hmac, KeyInit, Mac};
    use sha2::{Digest, Sha256};
    fn mac(key: &[u8], text: &str) -> Vec<u8> {
        let mut value = Hmac::<Sha256>::new_from_slice(key).unwrap();
        value.update(text.as_bytes());
        value.finalize().into_bytes().to_vec()
    }
    let fields = names
        .split(';')
        .map(|name| {
            let value = request
                .headers()
                .get_all(name)
                .iter()
                .map(|value| {
                    value
                        .to_str()
                        .unwrap()
                        .split([' ', '\t'])
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .collect::<Vec<_>>()
                .join(",");
            format!("{name}:{value}\n")
        })
        .collect::<String>();
    let timestamp = request.header("x-amz-date").unwrap();
    let date = &timestamp[..8];
    let payload = hex::encode(Sha256::digest(request.body().unwrap_or(b"")));
    assert_eq!(
        request.header("x-amz-content-sha256"),
        Some(payload.as_str())
    );
    let canonical = format!(
        "{}\n{path}\n{query}\n{fields}\n{names}\n{payload}",
        request.method()
    );
    let scope = format!("{date}/{region}/s3/aws4_request");
    let text = format!(
        "AWS4-HMAC-SHA256\n{timestamp}\n{scope}\n{}",
        hex::encode(Sha256::digest(canonical))
    );
    let key = mac(b"AWS4secret", date);
    let key = mac(&key, region);
    let key = mac(&key, "s3");
    let key = mac(&key, "aws4_request");
    let expected = format!(
        "AWS4-HMAC-SHA256 Credential=ACCESS/{scope}, SignedHeaders={names}, Signature={}",
        hex::encode(mac(&key, &text))
    );
    assert!(request.header("authorization") == Some(expected.as_str()));
}

#[tokio::test(start_paused = true)]
async fn c12_9_s3_reads_an_xml_bucket_listing_and_signs_the_global_request() {
    let script = Script::new();
    script.answer(200).body("<ListAllMyBucketsResult><ContinuationToken>next</ContinuationToken></ListAllMyBucketsResult>");
    let store = ObjectStore::new(
        "eu-west-1",
        Credentials::new("ACCESS", "secret"),
        Vec::<Base>::new(),
    )
    .with_transport(script.clone());
    let page = store.list_buckets().send().await.unwrap();
    assert_eq!(page.continuation_token.as_deref(), Some("next"));
    let requests = script.requests();
    assert_eq!(requests.len(), 1);
    verify_s3(
        &requests[0],
        "/",
        "",
        "us-east-1",
        "host;x-amz-content-sha256;x-amz-date",
    );
}

#[tokio::test(start_paused = true)]
async fn c12_10_hmac_recipe_signs_and_refuses_a_foreign_origin() {
    use hmac::{Hmac, KeyInit, Mac};
    use sha2::{Digest, Sha256};
    let script = Script::new();
    script.answer(200).body("balance");
    let origin: Base = "https://vault.test".parse().unwrap();
    let vault = Vault::new(origin, "secret").with_transport(script.clone());
    assert_eq!(vault.balance().await.unwrap(), "balance");
    let requests = script.requests();
    assert_eq!(requests.len(), 1);
    let time = requests[0].header("x-proof-time").unwrap();
    let text = format!(
        "{time}\nGET\n/balance\n\n{}",
        hex::encode(Sha256::digest(b""))
    );
    let mut mac = Hmac::<Sha256>::new_from_slice(b"secret").unwrap();
    mac.update(text.as_bytes());
    assert!(requests[0].header("x-proof").unwrap() == hex::encode(mac.finalize().into_bytes()));
    assert!(requests[0].headers()["x-proof"].is_sensitive());
    let error = vault
        .balance()
        .edit_request(|request| {
            *request.url_mut() = "https://foreign.test/balance".parse().unwrap()
        })
        .await
        .unwrap_err();
    assert!(matches!(
        SignError::find(&error),
        Some(SignError::WrongOrigin)
    ));
    assert_eq!(script.requests().len(), 1);
}

#[test]
fn c12_9_hmac_recipe_matches_its_compiled_source() {
    let page = include_str!("../../../docs/guide/credentials.md");
    let recipe = include_str!("../src/credential_signing.rs");
    assert!(page.contains(&format!("```rust\n{}\n```", recipe.trim())));
}

#[tokio::test(start_paused = true)]
async fn c12_9_s3_adapter_keeps_its_signing_clock_across_attempts() {
    let origin: Base = "https://bucket.s3.eu-west-1.amazonaws.com".parse().unwrap();
    let script = Script::new();
    script.answer(503).header("retry-after", "2");
    script.answer(200);
    let store = ObjectStore::new(
        "eu-west-1",
        Credentials::new("ACCESS", "secret"),
        vec![origin.clone()],
    )
    .with_transport(script.clone())
    .with_retry(concord::Retry::standard().tries(2));
    store.bucket(origin).head().await.unwrap();
    let requests = script.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(
        requests
            .iter()
            .map(|r| r.info().number())
            .collect::<Vec<_>>(),
        [1, 2]
    );
    assert_ne!(
        requests[0].header("x-amz-date"),
        requests[1].header("x-amz-date")
    );
    assert!(requests[0].header("authorization") != requests[1].header("authorization"));
}
