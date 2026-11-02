//! U16 / U29 (R09): downstream representations (NDJSON, form) and a native multipart body.

mod support;

use concord::{Decode, Encode, ErrorKind};
use concord_example_native::api::StockRow;
use concord_example_native::bodies::{FilePart, import_stock, send_feedback, upload_file};
use concord_example_native::repr::{Form, Ndjson};
use support::{Reply, Server, inventory_at};

fn row(sku: &str, qty: u32) -> StockRow {
    StockRow {
        sku: sku.into(),
        qty,
    }
}

#[tokio::test]
async fn ndjson_round_trip_on_the_wire() {
    let server = Server::reply(
        Reply::new(200)
            .header("content-type", "application/x-ndjson")
            .body("{\"sku\":\"A-1\",\"qty\":5}\n\n{\"sku\":\"B 2\",\"qty\":0}\r\n"),
    )
    .await;
    let api = inventory_at(server.addr, "Bearer t");
    let result = import_stock(&api, vec![row("A-1", 5), row("B 2", 0)])
        .await
        .unwrap();
    assert_eq!(result, [row("A-1", 5), row("B 2", 0)]);
    let request = server.single();
    assert_eq!(
        (request.method.as_str(), request.target.as_str()),
        ("POST", "/v1/stock/import")
    );
    assert_eq!(request.header("content-type"), Some("application/x-ndjson"));
    assert_eq!(
        request.body,
        b"{\"sku\":\"A-1\",\"qty\":5}\n{\"sku\":\"B 2\",\"qty\":0}\n"
    );
}

#[tokio::test]
async fn ndjson_decode_error_is_content_free() {
    let server =
        Server::reply(Reply::new(200).body("{\"sku\":\"A\",\"qty\":1}\n{\"sku\":\"TOPSECRET\"}\n"))
            .await;
    let api = inventory_at(server.addr, "Bearer t");
    let error = import_stock(&api, vec![row("A", 1)]).await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Decode);
    let shown = error.to_string();
    assert!(
        shown.contains("NDJSON line 2 is not a valid record"),
        "{shown}"
    );
    assert!(!shown.contains("TOPSECRET"), "{shown}");
    // The detailed serde error is available on request, unredacted.
    assert!(error.body_source().is_some());
}

#[test]
fn representations_are_usable_directly() {
    assert_eq!(
        <Ndjson as Encode<Vec<StockRow>>>::CONTENT_TYPE,
        Some("application/x-ndjson")
    );
    let rows: Vec<StockRow> =
        <Ndjson as Decode<Vec<StockRow>>>::decode("{\"sku\":\"x\",\"qty\":1}".into()).unwrap();
    assert_eq!(rows, [row("x", 1)]);
    assert_eq!(
        <Form as Encode<Vec<(String, String)>>>::CONTENT_TYPE,
        Some("application/x-www-form-urlencoded")
    );
}

#[tokio::test]
async fn form_body_keeps_order_repeats_and_encoding() {
    let server = Server::reply(Reply::new(204)).await;
    let api = inventory_at(server.addr, "Bearer t");
    let fields = vec![
        ("topic".to_owned(), "stock & pricing".to_owned()),
        ("tag".to_owned(), "a/b".to_owned()),
        ("tag".to_owned(), "é=1".to_owned()),
    ];
    send_feedback(&api, fields).await.unwrap();
    let request = server.single();
    assert_eq!(request.target, "/v1/feedback");
    assert_eq!(
        request.header("content-type"),
        Some("application/x-www-form-urlencoded")
    );
    assert_eq!(
        String::from_utf8(request.body).unwrap(),
        "topic=stock+%26+pricing&tag=a%2Fb&tag=%C3%A9%3D1"
    );
}

#[tokio::test]
async fn multipart_upload_through_the_native_body() {
    let server = Server::reply(Reply::json(201, r#"{"id":11,"name":"report.csv"}"#)).await;
    let api = inventory_at(server.addr, "Bearer secret-token");
    let content = b"sku,qty\r\nA-1,5\r\n";
    let item = upload_file(
        &api,
        FilePart {
            field: "file",
            file_name: "report \"q3\".csv",
            content_type: "text/csv",
            content,
        },
    )
    .await
    .unwrap();
    assert_eq!(item.id, 11);

    let request = server.single();
    assert_eq!(
        (request.method.as_str(), request.target.as_str()),
        ("POST", "/v1/uploads")
    );
    assert_eq!(request.header("authorization"), Some("Bearer secret-token"));
    let content_type = request.header("content-type").unwrap();
    let boundary = content_type
        .strip_prefix("multipart/form-data; boundary=")
        .expect("multipart content type");
    let expected = format!(
        "--{boundary}\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"report %22q3%22.csv\"\r\n\
         Content-Type: text/csv\r\n\r\n\
         sku,qty\r\nA-1,5\r\n\r\n\
         --{boundary}--\r\n"
    );
    assert_eq!(String::from_utf8(request.body).unwrap(), expected);
}

#[tokio::test]
async fn multipart_boundary_never_occurs_in_the_content() {
    let server = Server::reply(Reply::json(201, r#"{"id":1,"name":"x"}"#)).await;
    let api = inventory_at(server.addr, "Bearer t");
    // Content that contains the first candidate boundary.
    let content = b"--concord-example-boundary-0000000000000000--";
    upload_file(
        &api,
        FilePart {
            field: "f",
            file_name: "x",
            content_type: "text/plain",
            content,
        },
    )
    .await
    .unwrap();
    let request = server.single();
    let boundary = request
        .header("content-type")
        .unwrap()
        .strip_prefix("multipart/form-data; boundary=")
        .unwrap()
        .to_owned();
    assert_eq!(boundary, "concord-example-boundary-0000000000000001");
    let body = String::from_utf8(request.body).unwrap();
    assert_eq!(body.matches(&*boundary).count(), 2);
}
