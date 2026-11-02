//! Loopback compatibility evidence for resource_key.
mod support;
use concord_example_compat::Resources;
use support::{http, Reply, Server};

#[tokio::test]
async fn segments_and_literal_percent_escapes() {
    let server = Server::reply(Reply::new(204)).await;
    let api = Resources::new(http()).with_base_url(server.url()).unwrap();
    for key in ["docs/2026/report.md", "docs/encoded%2Fname.md"] {
        api.write(key).send().await.unwrap();
        let (client, mut request, decoder) =
            api.write("placeholder").prepare().unwrap().into_parts();
        request
            .url_mut()
            .path_segments_mut()
            .unwrap()
            .pop()
            .extend(key.split('/'));
        decoder.finish(client.execute(request).await).await.unwrap();
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
