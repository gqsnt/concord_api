//! `refused_addr()`: the guarantees the "connection refused" tests of every SDK rely on, checked
//! on each platform the suite runs on (Linux and native Windows in CI).

use std::io::ErrorKind;
use std::time::Duration;

use concord_example_testserver::{Reply, TestServer, refused_addr, refused_url};

#[test]
fn a_connection_to_the_refused_port_is_refused() {
    let error = std::net::TcpStream::connect_timeout(&refused_addr(), Duration::from_secs(30))
        .expect_err("nothing may accept a connection on the refused port");
    assert_eq!(error.kind(), ErrorKind::ConnectionRefused, "{error:?}");
}

#[tokio::test]
async fn a_request_to_the_refused_port_is_a_connect_error() {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap();
    let error = client.get(refused_url()).send().await.unwrap_err();
    assert!(error.is_connect(), "{error:?}");
    assert!(!error.is_timeout(), "{error:?}");
}

#[tokio::test]
async fn no_listener_can_bind_the_refused_port() {
    let addr = refused_addr();
    let error = std::net::TcpListener::bind(addr).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::AddrInUse, "{error:?}");
    let error = tokio::net::TcpListener::bind(addr).await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::AddrInUse, "{error:?}");
}

/// Servers started meanwhile (each asks for port 0) are never given the refused port, and it
/// still refuses afterwards. (Many allocations, not a proof: the guarantee is `AddrInUse` above.)
#[tokio::test]
async fn servers_started_meanwhile_never_get_the_refused_port() {
    let addr = refused_addr();
    let mut servers = Vec::new();
    for _ in 0..64 {
        let server = TestServer::start(|_| Reply::new(204)).await;
        assert_ne!(server.addr().port(), addr.port());
        servers.push(server);
    }
    for server in servers {
        server.shutdown().await;
    }
    let error = tokio::net::TcpStream::connect(addr).await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::ConnectionRefused, "{error:?}");
}
