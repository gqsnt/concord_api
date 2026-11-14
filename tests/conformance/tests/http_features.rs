//! C7.1: Cargo enables HTTP capabilities; a client opts into decompression itself.

#[cfg(any(feature = "gzip", feature = "deflate"))]
mod compressed {
    use concord::bytes::Bytes;
    use concord_testkit::Server;
    use std::io::Write;

    concord::api! {
        client Encoded {
            base "https://encoded.example";
            read concord::Binary;
            GET bytes() -> Bytes;
        }
    }

    #[cfg(feature = "gzip")]
    fn gzip(body: &[u8]) -> Vec<u8> {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
        encoder.write_all(body).unwrap();
        encoder.finish().unwrap()
    }

    #[cfg(feature = "gzip")]
    #[tokio::test]
    async fn c7_1_gzip_feature_alone_preserves_headers_and_compressed_bytes() {
        let encoded = gzip(b"the original bytes");
        let server = Server::start().await;
        server
            .on("GET", "/")
            .reply(200)
            .header("content-encoding", "gzip")
            .body(encoded.clone());
        let http = concord::Http::with(|builder| builder.no_proxy()).unwrap();
        let client = Encoded::new().with_transport(http).with_base(server.url());
        let answer = client.bytes().send_full().await.unwrap();
        assert_eq!(answer.value().as_ref(), encoded.as_slice());
        assert_eq!(answer.headers().get("content-encoding"), Some("gzip"));
        assert_eq!(server.requests()[0].header("accept-encoding"), None);
    }

    #[cfg(feature = "gzip")]
    #[tokio::test]
    async fn c7_1_gzip_opt_in_sends_accept_encoding_and_decodes_the_body() {
        let body = b"decoded gzip bytes";
        let server = Server::start().await;
        server
            .on("GET", "/")
            .reply(200)
            .header("content-encoding", "gzip")
            .body(gzip(body));
        let http = concord::Http::with(|builder| builder.no_proxy().gzip(true)).unwrap();
        let client = Encoded::new().with_transport(http).with_base(server.url());
        assert_eq!(client.bytes().send().await.unwrap().as_ref(), body);
        let requests = server.requests();
        assert!(requests[0]
            .header("accept-encoding")
            .unwrap()
            .split(',')
            .any(|name| name.trim() == "gzip"));
    }

    #[cfg(feature = "deflate")]
    #[tokio::test]
    async fn c7_1_deflate_opt_in_sends_accept_encoding_and_decodes_the_body() {
        let body = b"decoded deflate bytes";
        let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::best());
        encoder.write_all(body).unwrap();
        let encoded = encoder.finish().unwrap();
        let server = Server::start().await;
        server
            .on("GET", "/")
            .reply(200)
            .header("content-encoding", "deflate")
            .body(encoded);
        let http = concord::Http::with(|builder| builder.no_proxy().deflate(true)).unwrap();
        let client = Encoded::new().with_transport(http).with_base(server.url());
        assert_eq!(client.bytes().send().await.unwrap().as_ref(), body);
        let requests = server.requests();
        assert!(requests[0]
            .header("accept-encoding")
            .unwrap()
            .split(',')
            .any(|name| name.trim() == "deflate"));
    }
}

#[cfg(feature = "socks")]
#[test]
fn c7_1_socks_proxy_is_accepted_without_connecting_to_a_proxy() {
    let proxy = concord::reqwest::Proxy::all("socks5://127.0.0.1:1080").unwrap();
    assert!(concord::Http::with(|builder| builder.no_proxy().proxy(proxy)).is_ok());
}
