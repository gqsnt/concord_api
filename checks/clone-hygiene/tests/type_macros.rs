//! F02: output types written as another crate's type macro are ordinary payloads, decoded by the
//! `response` format, for every delimiter and absolute or relative paths; a type macro body too.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use std::time::Duration;

use concord::reqwest;
use concord_clone_hygiene_check::type_macros::Macros;

/// One HTTP/1.1 exchange on localhost answering `body`; returns the base URL and the server thread.
fn serve(body: &'static str) -> (String, thread::JoinHandle<Vec<u8>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        let mut buf = [0u8; 1024];
        while !request.windows(4).any(|w| w == b"\r\n\r\n") {
            let n = socket.read(&mut buf).unwrap();
            assert!(n > 0, "request ended early");
            request.extend_from_slice(&buf[..n]);
        }
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        socket.write_all(response.as_bytes()).unwrap();
        request
    });
    (base, server)
}

#[tokio::test(flavor = "current_thread")]
async fn type_macro_outputs_decode() {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    for (endpoint, answer) in ["absolute", "brackets", "braces", "echo"]
        .into_iter()
        .zip(["17", "29", "31", "37"])
    {
        let (base, server) = serve(answer);
        let api = Macros::new(client.clone()).with_base_url(&base).unwrap();
        let shown = match endpoint {
            "absolute" => api.absolute().send().await.unwrap().to_string(),
            "brackets" => api.brackets().send().await.unwrap().to_string(),
            "braces" => api.braces().send().await.unwrap().to_string(),
            _ => api
                .echo(concord_clone_hygiene_d::Payload(5))
                .send()
                .await
                .unwrap()
                .to_string(),
        };
        assert_eq!(shown, answer, "{endpoint}");
        let request = String::from_utf8(server.join().unwrap()).unwrap();
        let line = if endpoint == "echo" {
            "POST / "
        } else {
            "GET /"
        };
        assert!(request.starts_with(line), "{request}");
    }
    // The decoded values have the macros' types (not merely the same text).
    let _: concord_clone_hygiene_a::Payload = {
        let (base, server) = serve("1");
        let api = Macros::new(client.clone()).with_base_url(&base).unwrap();
        let v = api.absolute().send().await.unwrap();
        server.join().unwrap();
        v
    };
}
