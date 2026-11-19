//! C7.1 and C5.4 against a real server whose routes sit behind `tower-http`'s compression
//! layer: such a server compresses only when the request asks for it. With the feature `gzip`
//! and `Http::with` turning the decoder on, the reader gets the decoded bytes and its limit
//! counts them; with the decoder off, nothing asks, and the reader gets the bytes as they
//! came, compressed when the description itself asks for gzip.

use crate::server::{Server, Wire};
use axum::routing::get;
use axum::Router;
use concord::bytes::Bytes;
use concord::Binary;
use tower_http::compression::CompressionLayer;

concord::api! {
    client Packed {
        base "http://packed.test";
        read Binary;
        GET plain() -> Bytes { path "packed"; }
        GET asking() -> Bytes { path "packed"; headers { "accept-encoding" = "gzip"; } }
        GET bounded() -> Bytes { path "packed"; read Binary.limit(1024); }
        GET bounded_asking() -> Bytes {
            path "packed";
            headers { "accept-encoding" = "gzip"; }
            read Binary.limit(1024);
        }
    }
}

/// 64 KiB of text that compresses well.
fn original() -> Vec<u8> {
    b"the same words, again and again. "
        .iter()
        .copied()
        .cycle()
        .take(64 * 1024)
        .collect()
}

async fn packing_server() -> Server {
    let router = Router::new()
        .route(
            "/packed",
            get(|| async { ([("content-type", "application/octet-stream")], original()) }),
        )
        .layer(CompressionLayer::new());
    Server::start(router).await
}

const GZIP_MAGIC: [u8; 2] = [0x1f, 0x8b];

#[cfg(feature = "gzip")]
async fn decoding_on(wire: Wire) {
    let server = packing_server().await;
    let packed = Packed::new()
        .with_transport(wire.http_with(|builder| builder.gzip(true)))
        .with_base(server.url());

    let answer = packed.plain().send_full().await.unwrap();
    assert_eq!(answer.value().as_ref(), original().as_slice());
    assert_eq!(answer.headers().get("content-encoding"), None);
    let asked = server.seen.all()[0].headers["accept-encoding"]
        .to_str()
        .unwrap()
        .to_owned();
    assert!(
        asked.split(',').any(|name| name.trim() == "gzip"),
        "{asked}"
    );

    // The limit counts the decoded bytes: 64 KiB is over 1 KiB, however few bytes came.
    let error = packed.bounded().await.unwrap_err();
    assert_eq!(error.kind(), concord::error::Kind::Mismatch);
    assert_eq!(error.status().map(|status| status.as_u16()), Some(200));
}

#[cfg(feature = "gzip")]
#[tokio::test]
async fn c7_1_with_gzip_turned_on_the_reader_gets_decoded_bytes_and_its_limit_counts_them_over_http1(
) {
    decoding_on(Wire::One).await;
}

#[cfg(feature = "gzip")]
#[tokio::test]
async fn c7_1_with_gzip_turned_on_the_reader_gets_decoded_bytes_and_its_limit_counts_them_over_http2(
) {
    decoding_on(Wire::Two).await;
}

async fn decoding_off(wire: Wire) {
    let server = packing_server().await;
    let packed = Packed::new()
        .with_transport(wire.http())
        .with_base(server.url());

    // Nothing asks for gzip, and the server sends the bytes as they are.
    let answer = packed.plain().send_full().await.unwrap();
    assert_eq!(answer.value().as_ref(), original().as_slice());
    assert_eq!(answer.headers().get("content-encoding"), None);
    assert!(server.seen.all()[0]
        .headers
        .get("accept-encoding")
        .is_none());

    // The description asks: the server compresses, and the reader gets what came.
    let answer = packed.asking().send_full().await.unwrap();
    assert_eq!(answer.headers().get("content-encoding"), Some("gzip"));
    assert_eq!(answer.value()[..2], GZIP_MAGIC);
    assert!(
        answer.value().len() < 1024,
        "{} bytes",
        answer.value().len()
    );

    // The limit counts the bytes as they came: the compressed body is under 1 KiB.
    let compressed = packed.bounded_asking().await.unwrap();
    assert_eq!(compressed[..2], GZIP_MAGIC);
}

#[tokio::test]
async fn c7_1_with_decoding_off_the_reader_gets_the_bytes_as_they_came_over_http1() {
    decoding_off(Wire::One).await;
}

#[tokio::test]
async fn c7_1_with_decoding_off_the_reader_gets_the_bytes_as_they_came_over_http2() {
    decoding_off(Wire::Two).await;
}
