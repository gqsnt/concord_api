#![cfg(feature = "loopback")]
use concord_test_support::tls::TlsServer;
use rustls::pki_types::{CertificateDer, ServerName};
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream};
use std::sync::Arc;
use std::time::Duration;

fn connect(server: &TlsServer) -> StreamOwned<ClientConnection, TcpStream> {
    let mut roots = RootCertStore::empty();
    roots
        .add(CertificateDer::from(server.certificate_der().to_vec()))
        .unwrap();
    let config = ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth();
    let socket = TcpStream::connect(server.address()).unwrap();
    socket
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    socket
        .set_write_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    socket.set_nodelay(true).unwrap();
    let connection =
        ClientConnection::new(Arc::new(config), ServerName::try_from("localhost").unwrap())
            .unwrap();
    StreamOwned::new(connection, socket)
}

fn hello(parts: &[&[u8]]) {
    let server = TlsServer::start().unwrap();
    server.queue_status(200).unwrap();
    let mut stream = connect(&server);
    for part in parts {
        stream.write_all(part).unwrap();
        stream.flush().unwrap();
    }
    let mut bytes = [0; 1024];
    let count = stream.read(&mut bytes).unwrap();
    assert!(bytes[..count].starts_with(b"HTTP/1.1 200"));
    server.wait_for_requests(1, Duration::from_secs(1)).unwrap();
    let records = server.requests();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].method(), "POST");
    assert_eq!(records[0].target(), "/fixture");
    assert_eq!(records[0].body(), b"hello");
}

#[test]
fn verified_tls_fixed_length_and_chunked_split_trailer_controls() {
    hello(&[b"POST /fixture HTTP/1.1\r\nHost: localhost\r\nContent-Length: 5\r\n\r\nhello"]);
    hello(&[b"POST /fixture HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\n\r\n"]);
    hello(&[
        b"POST /fixture HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n2;fixture=yes\r",
        b"\nhe\r\n3\r\nllo\r\n0\r\nX-Fixture: yes\r\n",
        b"\r\n",
    ]);
}

#[test]
fn verified_tls_pipelined_bytes_preserve_both_request_records() {
    let server = TlsServer::start().unwrap();
    server.queue_status(200).unwrap();
    server.queue_status(200).unwrap();
    let mut stream = connect(&server);
    stream.write_all(b"POST /one HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\n\r\nGET /two HTTP/1.1\r\n\r\n").unwrap();
    stream.flush().unwrap();
    let mut all = Vec::new();
    while all
        .windows(b"HTTP/1.1 200".len())
        .filter(|window| *window == b"HTTP/1.1 200")
        .count()
        < 2
    {
        let mut bytes = [0; 1024];
        let count = stream.read(&mut bytes).unwrap();
        assert!(count > 0);
        all.extend_from_slice(&bytes[..count]);
        assert!(all.len() < 16 * 1024);
    }
    server.wait_for_requests(2, Duration::from_secs(1)).unwrap();
    let records = server.requests();
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].body(), b"hello");
    assert_eq!(records[1].target(), "/two");
    assert_eq!(records[0].connection_id(), records[1].connection_id());
}

#[test]
fn verified_tls_ambiguous_malformed_truncated_or_capped_input_has_no_success_record() {
    let mut cases = vec![
        "POST / HTTP/1.1\r\nContent-Length: 5\r\nTransfer-Encoding: chunked\r\n\r\n0\r\n\r\n"
            .to_owned(),
        "POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n1\r\nxXX0\r\n\r\n".to_owned(),
        "POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n100001\r\n".to_owned(),
        "POST / HTTP/1.1\r\nContent-Length: 1048577\r\n\r\n".to_owned(),
        "POST / HTTP/1.1\r\nContent-Length: 5\r\n\r\nhell".to_owned(),
        "POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n0\r\nContent-Length: 5\r\n\r\n"
            .to_owned(),
    ];
    cases.push(format!(
        "POST / HTTP/1.1\r\nX-Fixture: {}\r\n\r\n",
        "x".repeat(16 * 1024)
    ));
    for wire in cases {
        let server = TlsServer::start().unwrap();
        let mut stream = connect(&server);
        let _ = stream.write_all(wire.as_bytes());
        let _ = stream.flush();
        let _ = stream.sock.shutdown(Shutdown::Write);
        let mut bytes = [0; 1024];
        match stream.read(&mut bytes) {
            Ok(0) | Err(_) => {}
            Ok(count) => panic!("invalid framing got a response: {:?}", &bytes[..count]),
        }
        assert!(
            server.requests().is_empty(),
            "invalid request must not record empty success"
        );
    }
}
