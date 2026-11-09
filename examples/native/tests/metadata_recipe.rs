//! W2 (M01–M05): the metadata-preserving recipe (`src/metadata.rs`) against loopback fixtures,
//! with synthetic secrets only.

mod support;

use std::net::SocketAddr;
use std::time::Duration;

use concord::reqwest::header::{ETAG, HeaderName, HeaderValue, SET_COOKIE};
use concord::reqwest::{self, StatusCode};
use concord::{Decode, ErrorKind, Json, Prepared};
use concord_example_native::api::{Inventory, InventoryConfig, Item};
use concord_example_native::metadata::{
    ITEM_META, MetaError, execute, item_with_meta, send_with_meta,
};
use support::{Reply, Server, inventory_at};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;

const LIMIT: Duration = Duration::from_secs(3);
const ITEM: &str = r#"{"id":7,"name":"Pen"}"#;

fn keep() -> [HeaderName; 4] {
    [
        ETAG,
        HeaderName::from_static("x-request-id"),
        HeaderName::from_static("x-total-count"),
        SET_COOKIE,
    ]
}

fn no_edit(_: &mut reqwest::Request) {}

fn http() -> reqwest::Client {
    reqwest::Client::builder().no_proxy().build().unwrap()
}

fn api_at(addr: SocketAddr) -> Inventory {
    Inventory::new(
        http(),
        InventoryConfig {
            key: "Bearer t".into(),
        },
    )
    .with_base_url(format!("http://{addr}/v1"))
    .unwrap()
}

/// Every `Display`/`Debug` rendering of an error and of its whole `source()` chain.
fn renderings(error: &(dyn std::error::Error + 'static)) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = Some(error);
    while let Some(e) = current {
        out.push(format!("{e}"));
        out.push(format!("{e:?}"));
        out.push(format!("{e:#?}"));
        current = e.source();
    }
    out
}

fn assert_absent(texts: &[String], secrets: &[&str]) {
    for text in texts {
        for secret in secrets {
            assert!(!text.contains(secret), "{secret} leaked in: {text}");
        }
    }
}

/// A server that sends the response head at once and the body only when released, recording
/// the request head it received.
async fn withholding(
    head: &'static str,
    body: &'static [u8],
) -> (
    SocketAddr,
    oneshot::Sender<()>,
    tokio::task::JoinHandle<String>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let (release, wait) = oneshot::channel::<()>();
    let task = tokio::spawn(async move {
        let (mut socket, _) = tokio::time::timeout(LIMIT, listener.accept())
            .await
            .unwrap()
            .unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            let n = socket.read(&mut byte).await.unwrap();
            assert_eq!(n, 1);
            request.push(byte[0]);
        }
        let head = format!("{head}\r\ncontent-length: {}\r\n\r\n", body.len());
        socket.write_all(head.as_bytes()).await.unwrap();
        socket.flush().await.unwrap();
        if tokio::time::timeout(LIMIT * 2, wait)
            .await
            .is_ok_and(|r| r.is_ok())
        {
            let _ = socket.write_all(body).await;
            let _ = socket.flush().await;
        }
        String::from_utf8(request).unwrap()
    });
    (addr, release, task)
}

/// M01: the caller's native edit (an extra header and a timeout) reaches the one request, the
/// selected metadata is captured and the endpoint's decoder produces the typed value.
#[tokio::test]
async fn m01_native_edit_one_execution_metadata_and_typed_value() {
    let server = Server::reply(
        Reply::json(200, ITEM)
            .header("etag", "\"v7\"")
            .header("x-request-id", "srv-1")
            .header("x-total-count", "42"),
    )
    .await;
    let api = inventory_at(server.addr, "Bearer t");

    let got = send_with_meta(api.item(7).prepare().unwrap(), &keep(), |request| {
        *request.timeout_mut() = Some(LIMIT);
        request
            .headers_mut()
            .insert("x-trace", HeaderValue::from_static("trace-1"));
    })
    .await
    .unwrap();

    assert_eq!(
        got.value,
        Item {
            id: 7,
            name: "Pen".into()
        }
    );
    assert_eq!(got.meta.status(), StatusCode::OK);
    assert_eq!(got.meta.raw_header(ETAG).unwrap(), "\"v7\"");
    assert_eq!(got.meta.raw_header("x-request-id").unwrap(), "srv-1");
    let count: u64 = got
        .meta
        .raw_header("x-total-count")
        .unwrap()
        .to_str()
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(count, 42);
    assert_eq!(got.meta.raw_header(SET_COOKIE), None, "absent header");

    let request = server.single(); // exactly one execution
    assert_eq!(request.target, "/v1/items/7");
    assert_eq!(request.header("x-trace"), Some("trace-1"));
    assert_eq!(
        request.header("authorization"),
        Some("Bearer t"),
        "declared header kept"
    );

    // The packaged form: the caller's request id is sent, the server's kept with the ETag; a
    // header outside the selection is not kept.
    let got = item_with_meta(&api, 7, HeaderValue::from_static("req-2"))
        .await
        .unwrap();
    assert_eq!(got.value.name, "Pen");
    assert_eq!(got.meta.raw_header(ETAG).unwrap(), "\"v7\"");
    assert_eq!(got.meta.raw_header("x-request-id").unwrap(), "srv-1");
    assert_eq!(got.meta.raw_header("x-total-count"), None);
    assert_eq!(ITEM_META.len(), 2);
    let all = server.requests();
    assert_eq!(all.len(), 2);
    assert_eq!(all[1].header("x-request-id"), Some("req-2"));
}

/// M01: the metadata is captured from the head while the body has not even been sent; the body
/// is read only by `decode`.
#[tokio::test]
async fn m01_metadata_is_captured_before_the_body_is_read() {
    let (addr, release, server) = withholding(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\netag: \"v3\"\r\nconnection: close",
        ITEM.as_bytes(),
    )
    .await;
    let api = api_at(addr);
    let received = tokio::time::timeout(
        LIMIT,
        execute(api.item(7).prepare().unwrap(), &keep(), |request| {
            request
                .headers_mut()
                .insert("x-trace", HeaderValue::from_static("trace-2"));
        }),
    )
    .await
    .expect("the head is enough: the withheld body is not awaited")
    .unwrap();
    assert_eq!(received.meta().status(), StatusCode::OK);
    assert_eq!(received.meta().raw_header(ETAG).unwrap(), "\"v3\"");

    release.send(()).unwrap();
    let got = tokio::time::timeout(LIMIT, received.decode())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(got.value.name, "Pen");
    assert_eq!(got.meta.raw_header(ETAG).unwrap(), "\"v3\"");
    let head = server.await.unwrap();
    assert!(head.contains("\r\nx-trace: trace-2\r\n"), "{head}");
}

/// M01/M03: the edited timeout applies at both stages. Before the head: `Transport`, no
/// metadata. While the body is withheld: `Transport` after the head, its metadata kept.
#[tokio::test]
async fn edited_timeout_before_and_after_the_head() {
    let server = Server::reply(Reply::json(200, ITEM).delay(Duration::from_secs(3))).await;
    let api = inventory_at(server.addr, "Bearer t");
    let error = send_with_meta(api.item(7).prepare().unwrap(), &keep(), |request| {
        *request.timeout_mut() = Some(Duration::from_millis(200));
    })
    .await
    .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Transport);
    assert!(error.error().transport().unwrap().is_timeout());
    assert!(error.meta().is_none(), "no head arrived");

    let (addr, _release, _server) = withholding(
        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\netag: \"v4\"",
        ITEM.as_bytes(),
    )
    .await;
    let api = api_at(addr);
    let error = send_with_meta(api.item(7).prepare().unwrap(), &keep(), |request| {
        *request.timeout_mut() = Some(Duration::from_millis(300));
    })
    .await
    .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Transport);
    assert!(error.error().transport().unwrap().is_timeout());
    let meta = error.meta().expect("the head arrived before the timeout");
    assert_eq!(meta.status(), StatusCode::OK);
    assert_eq!(meta.raw_header(ETAG).unwrap(), "\"v4\"");
    assert!(
        error.error().response().is_none(),
        "the body read was abandoned"
    );
}

const SECRET: &str = "SYNTHETIC-SECRET";

/// A loopback port nobody listens on (held for the whole process): connecting is refused,
/// deterministically.
fn refused_port() -> SocketAddr {
    support::refused_addr()
}

/// A signed URL: the edit appends a secret query value.
fn sign(request: &mut reqwest::Request) {
    request
        .url_mut()
        .query_pairs_mut()
        .append_pair("token", SECRET);
}

/// The archived campaign helper's shape (`Pending::receive`): `client.execute(request).await?`
/// hands back reqwest's own error, skipping Concord's conversion. The control below.
async fn archived_receive<T>(
    prepared: Prepared<T>,
    edit: impl FnOnce(&mut reqwest::Request),
) -> Result<reqwest::Response, reqwest::Error> {
    let (client, mut request, _decoder) = prepared.into_parts();
    edit(&mut request);
    let response = client.execute(request).await?;
    Ok(response)
}

/// M02: a transport failure on a URL carrying a synthetic secret goes through Concord's
/// conversion: neither the wrapper's `Display`/`Debug` nor any `source()` chain restores the URL.
/// The raw native error stays reachable only through the explicit `transport()` boundary. The
/// archived pattern, on the same request, prints the secret.
#[tokio::test]
async fn m02_transport_failure_is_converted_and_does_not_print_the_url() {
    let addr = refused_port();
    let api = api_at(addr);

    let error = send_with_meta(api.item(7).prepare().unwrap(), &keep(), sign)
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Transport);
    assert!(error.meta().is_none(), "no response head");
    assert_eq!(error.error().endpoint(), Some("Inventory.item"));
    assert_eq!(
        error.to_string(),
        "GET Inventory.item: transport error (connect)"
    );

    let mut texts = renderings(&error);
    texts.extend(renderings(error.error()));
    assert_eq!(
        std::error::Error::source(&error).map(|e| e.to_string()),
        None,
        "no source chain"
    );
    assert_absent(&texts, &[SECRET, "token="]);

    // The deliberate boundary: the native error, its URL removed at the top level; its own
    // source chain is reqwest's/hyper's and is not redacted by Concord.
    let native = error.error().transport().expect("explicit native access");
    assert!(native.is_connect());
    assert!(native.url().is_none());

    // Control: the archived pattern returns the raw reqwest error, which prints the URL.
    let raw = archived_receive(api.item(7).prepare().unwrap(), sign)
        .await
        .unwrap_err();
    let (display, debug) = (raw.to_string(), format!("{raw:?}"));
    assert!(display.contains(SECRET), "control display: {display}");
    assert!(debug.contains(SECRET), "control debug: {debug}");
    eprintln!("M02 recipe display: {error}");
    eprintln!("M02 recipe debug:   {error:?}");
    eprintln!("M02 control display (archived pattern): {display}");
}

/// M03: a status rejection and a head (metadata) rejection keep the original response unread
/// inside the Concord error; the metadata is captured too.
#[tokio::test]
async fn m03_status_and_head_rejections_keep_the_unread_response() {
    let server = Server::reply(
        Reply::new(409)
            .header("content-type", "text/html")
            .header("x-request-id", "r-409")
            .body("<p>conflict</p>"),
    )
    .await;
    let api = inventory_at(server.addr, "Bearer t");
    let error = send_with_meta(api.item(7).prepare().unwrap(), &keep(), no_edit)
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Status);
    let meta = error.meta().unwrap();
    assert_eq!(meta.status(), StatusCode::CONFLICT);
    assert_eq!(meta.raw_header("x-request-id").unwrap(), "r-409");
    assert_eq!(error.error().status(), Some(StatusCode::CONFLICT));
    let (error, meta) = error.into_parts();
    let response = error.into_response().unwrap();
    assert_eq!(response.text().await.unwrap(), "<p>conflict</p>");
    assert_eq!(meta.unwrap().status(), StatusCode::CONFLICT);

    // A JSON endpoint answered `text/html`: `Json` refuses the head, the body stays unread.
    let server = Server::reply(
        Reply::new(200)
            .header("content-type", "text/html")
            .header("x-request-id", "r-html")
            .body(ITEM),
    )
    .await;
    let api = inventory_at(server.addr, "Bearer t");
    let error = send_with_meta(api.item(7).prepare().unwrap(), &keep(), no_edit)
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Metadata);
    assert_eq!(error.meta().unwrap().status(), StatusCode::OK);
    assert_eq!(
        error.meta().unwrap().raw_header("x-request-id").unwrap(),
        "r-html"
    );
    let response = error.into_parts().0.into_response().unwrap();
    assert_eq!(response.headers()["content-type"], "text/html");
    let item = <Json as Decode<Item>>::decode_body((), response.bytes().await.unwrap()).unwrap();
    assert_eq!(item.name, "Pen");
}

/// M03: a failed body read (`Transport`, the head received) and a failed decoding (`Decode`)
/// are told apart; after either, the head metadata is available and the body is not.
#[tokio::test]
async fn m03_body_read_failure_and_decode_failure_keep_only_the_head() {
    // The server declares 100 bytes, sends 9 and closes.
    let server = Server::reply(
        Reply::json(200, r#"{"id":7,"#)
            .header("etag", "\"t1\"")
            .truncated(100),
    )
    .await;
    let api = inventory_at(server.addr, "Bearer t");
    let error = send_with_meta(api.item(7).prepare().unwrap(), &keep(), no_edit)
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Transport);
    assert_eq!(
        error.to_string(),
        "GET Inventory.item: transport error (reading the response body)"
    );
    let native = error.error().transport().unwrap();
    // reqwest 0.13 reports a truncated body read through `bytes()` as its own `is_decode()`
    // (hyper's `IncompleteBody`); another reqwest release may call it `is_body()`. Either way it is
    // Concord's `Transport`, not Concord's `Decode`.
    assert!(
        (native.is_decode() || native.is_body()) && !native.is_timeout(),
        "{native:?}"
    );
    let meta = error.meta().expect("the head was received");
    assert_eq!(meta.status(), StatusCode::OK);
    assert_eq!(meta.raw_header(ETAG).unwrap(), "\"t1\"");
    assert!(error.error().response().is_none());
    let (error, _) = error.into_parts();
    let error = error.into_response().unwrap_err(); // no response to take: the body is gone
    assert_eq!(error.kind(), ErrorKind::Transport);

    let server = Server::reply(Reply::json(200, "not json").header("etag", "\"d1\"")).await;
    let api = inventory_at(server.addr, "Bearer t");
    let error = send_with_meta(api.item(7).prepare().unwrap(), &keep(), no_edit)
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Decode);
    assert!(error.error().codec_error().is_some());
    assert!(error.error().transport().is_none());
    let meta = error.meta().expect("the head was received");
    assert_eq!(meta.raw_header(ETAG).unwrap(), "\"d1\"");
    assert!(error.error().response().is_none());
    let error = error.into_parts().0.into_response().unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Decode);
}

/// M04: hand-written `Debug` of the metadata, success and error wrappers shows no header value,
/// no URL and no payload; the explicit accessors return them.
#[tokio::test]
async fn m04_wrappers_do_not_print_values_and_accessors_return_them() {
    const COOKIE: &str = "SYNTHETIC-COOKIE";
    const URL_SECRET: &str = "SYNTHETIC-URL-SECRET";
    const PAYLOAD: &str = "SYNTHETIC-PAYLOAD";
    const UNKEPT: &str = "SYNTHETIC-UNKEPT";
    let secrets = [COOKIE, URL_SECRET, PAYLOAD, UNKEPT, "127.0.0.1"];
    let server = Server::start(|request| {
        let reply = if request.path().ends_with("/7") {
            Reply::json(200, r#"{"id":7,"name":"SYNTHETIC-PAYLOAD"}"#)
        } else if request.path().ends_with("/8") {
            Reply::json(200, "SYNTHETIC-PAYLOAD is not JSON")
        } else {
            Reply::text(409, "SYNTHETIC-PAYLOAD")
        };
        reply
            .header("set-cookie", "session=SYNTHETIC-COOKIE")
            .header("set-cookie", "theme=dark")
            .header("etag", "\"m4\"")
            .header("x-unkept", "SYNTHETIC-UNKEPT")
    })
    .await;
    let api = inventory_at(server.addr, "Bearer t");
    let edit = |request: &mut reqwest::Request| {
        request
            .url_mut()
            .query_pairs_mut()
            .append_pair("sig", URL_SECRET);
    };

    let received = execute(api.item(7).prepare().unwrap(), &keep(), edit)
        .await
        .unwrap();
    let received_debug = format!("{received:?}");
    let got = received.decode().await.unwrap();
    let texts = vec![
        received_debug,
        format!("{got:?}"),
        format!("{got:#?}"),
        format!("{:?}", got.meta),
        format!("{:#?}", got.meta),
    ];
    assert_absent(&texts, &secrets);
    // What `Debug` does show: the status and the kept header names.
    let shown = format!("{:?}", got.meta);
    assert!(shown.contains("status: 200"), "{shown}");
    assert!(
        shown.contains("\"set-cookie\"") && shown.contains("\"etag\""),
        "{shown}"
    );
    assert!(!shown.contains("x-unkept"), "{shown}");
    eprintln!("M04 success debug: {got:?}");

    // Explicit access returns everything that was kept.
    assert_eq!(got.value.name, PAYLOAD);
    assert_eq!(
        got.meta.raw_header(SET_COOKIE).unwrap(),
        "session=SYNTHETIC-COOKIE"
    );
    let cookies: Vec<_> = got.meta.raw_headers().get_all(SET_COOKIE).iter().collect();
    assert_eq!(cookies, ["session=SYNTHETIC-COOKIE", "theme=dark"]);
    assert_eq!(got.meta.raw_url().query(), Some("sig=SYNTHETIC-URL-SECRET"));
    assert_eq!(
        got.meta.raw_header("x-unkept"),
        None,
        "not selected, not kept"
    );

    // The error wrapper, after a decode failure (head kept) and a status failure (response kept).
    for id in [8, 9] {
        let error: MetaError = send_with_meta(api.item(id).prepare().unwrap(), &keep(), edit)
            .await
            .unwrap_err();
        assert!(matches!(
            error.kind(),
            ErrorKind::Decode | ErrorKind::Status
        ));
        assert_absent(&renderings(&error), &secrets);
        let meta = error.meta().unwrap();
        assert_eq!(
            meta.raw_header(SET_COOKIE).unwrap(),
            "session=SYNTHETIC-COOKIE"
        );
        assert!(meta.raw_url().as_str().contains(URL_SECRET));
        eprintln!("M04 error debug: {error:?}");
    }
}

/// M05: the recipe leaves the declared handling alone: an endpoint without output still drains
/// to `()` (and still detects a truncated body); ordinary `send()` is unchanged next to it.
#[tokio::test]
async fn m05_no_output_endpoint_drains_through_the_recipe() {
    let server = Server::reply(Reply::new(204).header("x-request-id", "fb-1")).await;
    let api = inventory_at(server.addr, "Bearer t");
    let fields = vec![("rating".to_owned(), "5".to_owned())];
    let got = send_with_meta(
        api.feedback(fields.clone()).prepare().unwrap(),
        &keep(),
        no_edit,
    )
    .await
    .unwrap();
    let () = got.value;
    assert_eq!(got.meta.status(), StatusCode::NO_CONTENT);
    assert_eq!(got.meta.raw_header("x-request-id").unwrap(), "fb-1");
    api.feedback(fields.clone()).send().await.unwrap();
    assert_eq!(server.requests().len(), 2);

    let server = Server::reply(Reply::new(200).body("0123456789").truncated(1000)).await;
    let api = inventory_at(server.addr, "Bearer t");
    let error = send_with_meta(api.feedback(fields).prepare().unwrap(), &keep(), no_edit)
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Transport);
    assert_eq!(error.meta().unwrap().status(), StatusCode::OK);
}

/// A build failure converted with `?` carries no metadata.
#[tokio::test]
async fn build_errors_convert_without_metadata() {
    let error = MetaError::from(
        Inventory::new(
            http(),
            InventoryConfig {
                key: "bad\nkey".into(),
            },
        )
        .item(1)
        .prepare()
        .unwrap_err(),
    );
    assert_eq!(error.kind(), ErrorKind::Build);
    assert!(error.meta().is_none());
}
