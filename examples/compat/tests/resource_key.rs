//! Loopback compatibility evidence for resource_key.
mod support;
use concord_example_compat::{write_resource_segments, Resources};
use support::{http, Reply, Server};

#[tokio::test]
async fn segments_and_literal_percent_escapes() {
    let server = Server::reply(Reply::new(204)).await;
    let api = Resources::new(http()).with_base_url(server.url()).unwrap();
    for key in ["docs/2026/report.md", "docs/encoded%2Fname.md"] {
        api.write(key).send().await.unwrap();
        write_resource_segments(&api, &key.split('/').collect::<Vec<_>>())
            .await
            .unwrap();
    }
    assert_eq!(
        server
            .requests()
            .iter()
            .map(|r| r.target.as_str())
            .collect::<Vec<_>>(),
        [
            "/objects/docs%2F2026%2Freport.md",
            "/objects/docs/2026/report.md",
            "/objects/docs%2Fencoded%252Fname.md",
            "/objects/docs/encoded%252Fname.md",
        ]
    );
}

#[tokio::test]
async fn native_recipe_rejects_ambiguous_items_and_encodes_data() {
    let server = Server::reply(Reply::new(204)).await;
    let api = Resources::new(http())
        .with_base_url(format!("{}/base", server.url()))
        .unwrap();
    for segments in [
        &[][..],
        &[""][..],
        &["."][..],
        &[".."][..],
        &["a", ""][..],
        &["\n"][..],
        &["x\ty"][..],
        &["\0"][..],
    ] {
        assert!(write_resource_segments(&api, segments).await.is_err());
    }
    assert!(server.requests().is_empty());
    write_resource_segments(&api, &["a/b", "%2F", "\\", "é"])
        .await
        .unwrap();
    assert_eq!(
        server.single().target,
        "/base/objects/a%2Fb/%252F/%5C/%C3%A9"
    );
}
