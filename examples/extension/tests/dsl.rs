//! The type-level CSV format in Concord's existing declaration language (CSV.md §8 item 2): an
//! ordinary alias, a direct generic path (`response Csv<HasHeader, Semicolon>;`), a
//! `Delimiter<0x1f>` alias, a provider profile (this crate's `Csv`), the media override around a
//! generic path, a finite request body and a one-shot streamed request body. No CSV-specific
//! grammar, derive or private API: the statements name the types by path, as any format.

use concord::csv::{Csv, Delimiter, HasHeader, NoHeader, Rows, Semicolon, StreamRows};
use concord::reqwest;
use concord::stream::{FromIter, from_iter};
use concord_example_fixture::{Recorded, Reply, Server};

/// One row of the file service.
#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Row {
    /// Identifier.
    pub id: u32,
    /// Name.
    pub name: String,
}

/// An ordinary Rust alias of a type-level dialect.
pub type UnitSeparated = Csv<NoHeader, Delimiter<0x1f>>;
/// The provider profile of this crate.
pub type CatalogCsv = concord_example_extension::Csv;
/// A one-shot row source with a nameable type, for a declared streaming body.
pub type RowSource = StreamRows<FromIter<std::vec::IntoIter<Result<Row, std::io::Error>>>>;

concord::api! {
    /// A file service whose formats are type-level CSV dialects.
    pub client Files() {
        base "https://files.example.test/v1";

        /// A direct generic format path.
        GET semicolon() -> Vec<Row> { path "semicolon"; response Csv<HasHeader, Semicolon>; }
        /// An alias with a control-byte delimiter.
        GET unit() -> Vec<Row> { path "unit"; response UnitSeparated; }
        /// A provider profile.
        GET catalog() -> Vec<concord_example_extension::CsvRow> { path "catalog"; response CatalogCsv; }
        /// The deliberate media override around a generic path.
        GET mislabelled() -> Vec<Row> {
            path "mislabelled";
            response concord::IgnoreContentType<Csv<HasHeader, Semicolon>>;
        }
        /// A finite body.
        POST upload(body rows: Vec<Row>) { path "upload"; request Csv<HasHeader, Semicolon>; }
        /// An iterator body with explicit columns (still finite).
        POST upload_tuples(body rows: Rows<Vec<(u32, String)>>) { path "tuples"; request Csv; }
        /// A one-shot streamed body.
        POST upload_stream(body rows: RowSource) { path "stream"; request UnitSeparated; }
    }
}

fn respond(request: &Recorded) -> Reply {
    let csv = |content_type: &str, body: &[u8]| {
        Reply::new(200)
            .header("content-type", content_type)
            .body(body)
    };
    match request.target.as_str() {
        "/v1/semicolon" => csv("text/csv; header=present", b"name;id\r\nLyon;1\r\n"),
        "/v1/unit" => csv("text/csv; header=absent", b"1\x1fLyon\n2\x1fZo\xc3\xab\n"),
        "/v1/catalog" => csv("text/csv", b"id,name\nitem-001,\"Note, A5\"\n# trailer\n"),
        "/v1/mislabelled" => csv("application/octet-stream", b"id;name\n7;x\n"),
        _ => Reply::new(204),
    }
}

async fn files(server: &Server) -> Files {
    Files::new(reqwest::Client::new())
        .with_base_url(format!("{}/v1", server.url()))
        .unwrap()
}

#[tokio::test]
async fn csv_formats_decode_through_declared_endpoints() {
    let server = Server::start(respond).await;
    let api = files(&server).await;

    assert_eq!(
        api.semicolon().send().await.unwrap(),
        [Row {
            id: 1,
            name: "Lyon".into()
        }]
    );
    assert_eq!(
        api.unit().send().await.unwrap(),
        [
            Row {
                id: 1,
                name: "Lyon".into()
            },
            Row {
                id: 2,
                name: "Zoë".into()
            }
        ]
    );
    let catalog = api.catalog().send().await.unwrap();
    assert_eq!(catalog[0].name, "Note, A5");
    assert_eq!(api.mislabelled().send().await.unwrap()[0].id, 7);

    // The format's `Accept` suggestion is sent.
    for request in server.requests() {
        assert_eq!(
            request.header("accept"),
            Some("text/csv"),
            "{}",
            request.target
        );
    }

    // Without the override, the same mislabelled response is a `Metadata` error.
    let reply = Server::reply(
        Reply::new(200)
            .header("content-type", "application/octet-stream")
            .body("id;name\n7;x\n"),
    )
    .await;
    let error = files(&reply).await.semicolon().send().await.unwrap_err();
    assert_eq!(error.kind(), concord::ErrorKind::Metadata);
    assert_eq!(error.endpoint(), Some("Files.semicolon"));
}

#[tokio::test]
async fn csv_formats_encode_declared_bodies() {
    let server = Server::start(respond).await;
    let api = files(&server).await;

    api.upload(vec![Row {
        id: 1,
        name: "a;b".into(),
    }])
    .send()
    .await
    .unwrap();
    api.upload_tuples(Rows::new(vec![(1, "x".to_owned())]).with_columns(["id", "name"]))
        .send()
        .await
        .unwrap();
    let rows = vec![
        Ok(Row {
            id: 1,
            name: "a".into(),
        }),
        Ok(Row {
            id: 2,
            name: "b".into(),
        }),
    ];
    let prepared = api
        .upload_stream(StreamRows::new(from_iter(rows)))
        .prepare()
        .unwrap();
    prepared.send().await.unwrap();

    let seen = server.requests();
    assert_eq!(
        seen[0].header("content-type"),
        Some("text/csv; header=present")
    );
    assert_eq!(seen[0].body, b"id;name\r\n1;\"a;b\"\r\n");
    assert!(!seen[0].chunked);
    assert_eq!(
        seen[1].header("content-type"),
        Some("text/csv; header=present")
    );
    assert_eq!(seen[1].body, b"id,name\r\n1,x\r\n");
    assert_eq!(
        seen[2].header("content-type"),
        Some("text/csv; header=absent")
    );
    assert_eq!(seen[2].body, b"1\x1fa\r\n2\x1fb\r\n");
    assert!(seen[2].chunked, "a lazy body is sent chunked");
}
