//! C5.4: decompression cannot bypass a reader's byte or allocation limit.
//! This binary runs one case so its process memory peak belongs to this read.
#![cfg(feature = "gzip")]

use concord::bytes::Bytes;
use concord::error::Kind;
use concord::MIB;
use concord_testkit::Server;
use std::io::Write;

concord::api! {
    client Compressed {
        base "https://compressed.example";
        read concord::Binary;
        GET bytes() -> Bytes;
    }
}

// Make the compressed fixture without ever allocating its expanded 20 MiB.
fn compressed_zeros(length: usize) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    let zeros = [0u8; 4096];
    for _ in 0..length / zeros.len() {
        encoder.write_all(&zeros).unwrap();
    }
    encoder.finish().unwrap()
}

#[cfg(target_os = "linux")]
fn peak_kib() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix("VmPeak:"))
        .and_then(|value| value.split_whitespace().next()?.parse().ok())
        .expect("the process memory peak")
}

#[tokio::test]
async fn c5_4_gzip_limit_counts_decoded_bytes_without_allocating_the_expanded_body() {
    let encoded = compressed_zeros(20 * MIB);
    assert!(encoded.len() < 64 * 1024);
    let server = Server::start().await;
    server
        .on("GET", "/")
        .reply(200)
        .header("content-encoding", "gzip")
        .body(compressed_zeros(4096));
    server
        .on("GET", "/")
        .reply(200)
        .header("content-encoding", "gzip")
        .body(encoded);
    let http = concord::Http::with(|builder| builder.no_proxy().gzip(true)).unwrap();
    let client = Compressed::new()
        .with_transport(http)
        .with_base(server.url());

    // Warm client, codec and connection setup before observing the collector.
    assert_eq!(client.bytes().send().await.unwrap().len(), 4096);
    #[cfg(target_os = "linux")]
    let before = peak_kib();
    let error = client.bytes().send().await.unwrap_err();
    #[cfg(target_os = "linux")]
    let growth = peak_kib().saturating_sub(before);

    assert_eq!(error.kind(), Kind::Mismatch);
    assert!(
        error
            .to_string()
            .contains("the body is larger than the limit"),
        "{error}"
    );
    assert!(error.body().map_or(0, <[u8]>::len) <= 64 * 1024);
    #[cfg(target_os = "linux")]
    assert!(
        growth < 20 * 1024,
        "reading a compressed body grew process virtual memory by {growth} KiB"
    );
}
