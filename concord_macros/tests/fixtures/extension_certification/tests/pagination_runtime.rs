#![forbid(unsafe_code)]

use std::io;
use std::time::Duration;

use runtime::advanced::SafeProxy;
use concord_fixture_extension_certification::{
    ExtensionApi, ExtensionConstructionProbe, ExtensionProvider, extension_pagination_probe_counts,
    reset_extension_pagination_probes,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::timeout;

const DEADLOCK_BOUND: Duration = Duration::from_secs(10);
const MAX_REQUEST_HEAD_BYTES: usize = 16 * 1024;

async fn request_target(stream: &mut TcpStream) -> io::Result<String> {
    let mut request = Vec::new();
    let mut chunk = [0_u8; 1024];
    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "loopback request ended before its head",
            ));
        }
        request.extend_from_slice(&chunk[..read]);
        if request.len() > MAX_REQUEST_HEAD_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "loopback request head exceeded its test bound",
            ));
        }
    }

    let request = std::str::from_utf8(&request)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "request head was not UTF-8"))?;
    request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .map(str::to_owned)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "request target was absent"))
}

async fn serve_page(stream: &mut TcpStream, body: &str) -> io::Result<()> {
    let response = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: application/x-concord-page\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.shutdown().await
}

async fn run_two_page_proxy(listener: TcpListener) -> Vec<String> {
    let mut targets = Vec::new();
    for body in ["1,2|continuation", "3|-"] {
        let (mut stream, _) = timeout(DEADLOCK_BOUND, listener.accept())
            .await
            .expect("pagination proxy timed out awaiting a request")
            .expect("pagination proxy accept");
        let target = timeout(DEADLOCK_BOUND, request_target(&mut stream))
            .await
            .expect("pagination proxy timed out reading a request")
            .expect("pagination proxy request target");
        timeout(DEADLOCK_BOUND, serve_page(&mut stream, body))
            .await
            .expect("pagination proxy timed out writing a response")
            .expect("pagination proxy response");
        targets.push(target);
    }
    targets
}

#[tokio::test]
async fn downstream_generated_controller_constructs_binds_and_collects_two_pages() {
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .expect("ephemeral pagination proxy");
    let address = listener.local_addr().expect("pagination proxy address");
    let server = tokio::spawn(run_two_page_proxy(listener));
    let proxy =
        SafeProxy::parse(format!("http://{address}")).expect("credential-free loopback proxy URL");

    let client = ExtensionApi::builder()
        .construction(ExtensionConstructionProbe::new())
        .session_provider(ExtensionProvider)
        .http(move |http| http.proxy(proxy))
        .build()
        .expect("downstream generated client");
    let pagination = client.pages().cursor("initial".to_owned()).paginate(10_u32);

    reset_extension_pagination_probes();
    let items = pagination
        .await
        .expect("public-only downstream pagination runtime");
    let targets = timeout(DEADLOCK_BOUND, server)
        .await
        .expect("pagination proxy task timed out")
        .expect("pagination proxy task");
    let probe_counts = extension_pagination_probe_counts();

    assert_eq!(items, [1, 2, 3]);
    assert_eq!(targets.len(), 2);
    assert!(targets[0].contains("cursor=initial"));
    assert!(targets[1].contains("cursor=continuation"));
    assert_eq!(probe_counts, (1, 2, 2));

    println!(
        "CONCORD_DOWNSTREAM_PAGINATION_RUNTIME_OK pages={} items={} construction={} validation={} next={}",
        targets.len(),
        items.len(),
        probe_counts.0,
        probe_counts.1,
        probe_counts.2
    );
}
