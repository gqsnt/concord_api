//! The outside operation against the fake Catalog service, with request counts: X01 (absent
//! from the SDK, public APIs only, no seed endpoint), X02 (base prefix and path data), X03
//! (independent contexts, headers on the wire), X04 (own representation, truthful identity),
//! X05 (native edit, transport failure).

use std::time::Duration;

use concord::bytes::Bytes;
use concord::reqwest::header::{ACCEPT, HeaderMap, HeaderValue};
use concord::reqwest::{self, StatusCode, Version};
use concord::{Decode, ErrorKind, ResponseHead};
use concord_example_extension::{Csv, CsvRow, EXPORT_OPERATION, export, export_parts, export_with};
use concord_example_fixture::{Reply, Server, catalog};
use concord_example_sdk::CatalogConfig;
use concord_example_sdk::context::{CatalogContext, ContextError, TenantContext};

/// The path data every encoding test uses: slash, question mark, percent escape, space, plus,
/// non-ASCII and a fragment marker.
const HARD_TENANT: &str = "acme/east?x=1%20+ü #";
/// Its wire form: one segment, every byte outside RFC 3986 unreserved percent-encoded.
const HARD_TENANT_WIRE: &str = "acme%2Feast%3Fx%3D1%2520%2B%C3%BC%20%23";

fn context(server: &Server, prefix: &str, key: &str) -> CatalogContext {
    CatalogContext::new(reqwest::Client::new(), CatalogConfig { key: key.into() })
        .with_base_url(format!("{}{prefix}", server.url()))
        .unwrap()
}

fn rows() -> Vec<CsvRow> {
    vec![
        CsvRow {
            id: "item-001".into(),
            name: "Notebook".into(),
        },
        CsvRow {
            id: "item-002".into(),
            name: "Pen".into(),
        },
    ]
}

/// The fixture's `x-tenant-echo`: the decoded tenant with every non-alphanumeric byte
/// percent-encoded.
fn echo_of(tenant: &str) -> String {
    tenant
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() {
                char::from(b).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

/// The raw (still encoded) path segment after `tenants` in a request target or URL path.
fn tenant_segment(path: &str) -> &str {
    let mut pieces = path.split('/');
    pieces.find(|p| *p == "tenants").expect("a tenant path");
    pieces.next().expect("a tenant segment")
}

// ---------------------------------------------------------------------------------------------
// X01: absent from the SDK, public APIs only, no seed endpoint.

/// The lines of a Rust source that are code, not comments.
fn code_lines(source: &str) -> impl Iterator<Item = &str> {
    source
        .lines()
        .map(str::trim)
        .filter(|l| !l.starts_with("//"))
}

#[test]
fn x01_the_sdk_declares_no_export_route() {
    let declaration = include_str!("../../sdk/src/lib.rs");
    let paths: Vec<&str> = code_lines(declaration)
        .filter(|l| l.contains("path \""))
        .collect();
    assert!(paths.len() >= 6, "{paths:#?}");
    assert!(paths.iter().all(|p| !p.contains("export")), "{paths:#?}");
}

#[test]
fn x01_the_extension_uses_no_hidden_api_and_no_declared_endpoint() {
    let sources = [
        ("src/lib.rs", include_str!("../src/lib.rs")),
        ("src/csv.rs", include_str!("../src/csv.rs")),
        ("src/error.rs", include_str!("../src/error.rs")),
    ];
    for (file, source) in sources {
        for line in code_lines(source) {
            for forbidden in [
                "__private",
                "Prepared",
                ".prepare(",
                ".send()",
                "into_parts",
                "catalog_tenant",
                ".catalog()",
            ] {
                assert!(
                    !line.contains(forbidden),
                    "{file}: `{forbidden}` in `{line}`"
                );
            }
        }
    }
}

#[tokio::test]
async fn x01_one_request_to_the_export_route_and_nothing_else() {
    let server = Server::start(catalog("/catalog/v1")).await;
    let acme = context(&server, "/catalog/v1", "k-ext").tenant("acme");

    assert_eq!(export(&acme, "items-2026").await.unwrap(), rows());

    let seen = server.single();
    assert_eq!(seen.method, "GET");
    assert_eq!(seen.target, "/catalog/v1/tenants/acme/exports/items-2026");
    assert_eq!(seen.header("accept"), Some("text/csv"));
    assert_eq!(server.connections(), 1);
}

// ---------------------------------------------------------------------------------------------
// X02: base path prefix and path data.

#[tokio::test]
async fn x02_a_base_path_prefix_is_kept() {
    let server = Server::start(catalog("/edge/catalog/v1")).await;
    for prefix in ["/edge/catalog/v1", "/edge/catalog/v1/"] {
        let acme = context(&server, prefix, "k-ext").tenant("acme");
        assert_eq!(export(&acme, "items-2026").await.unwrap(), rows());
    }
    let seen = server.requests();
    assert_eq!(seen.len(), 2);
    for request in seen {
        assert_eq!(
            request.target,
            "/edge/catalog/v1/tenants/acme/exports/items-2026"
        );
    }
}

#[tokio::test]
async fn x02_hard_tenant_and_export_data_stay_one_segment() -> Result<(), concord::Error> {
    let server = Server::start(catalog("/edge/catalog/v1")).await;
    let tenant = context(&server, "/edge/catalog/v1", "k-ext").tenant(HARD_TENANT);

    // Executed through the parts, to read the response headers before decoding.
    let (http, request, decoder) = export_parts(&tenant, "items-2026").unwrap();
    let response = match http.execute(request).await {
        Ok(response) => response,
        Err(e) => return Err(decoder.finish(Err(e)).await.unwrap_err()),
    };
    let echo = response.headers()["x-tenant-echo"]
        .to_str()
        .unwrap()
        .to_owned();
    let export_id = response.headers()["x-export-id"]
        .to_str()
        .unwrap()
        .to_owned();
    assert_eq!(decoder.decode(response).await?, rows());

    let seen = server.single();
    assert_eq!(
        seen.target,
        format!("/edge/catalog/v1/tenants/{HARD_TENANT_WIRE}/exports/items-2026")
    );
    assert_eq!(
        seen.segments(),
        [
            "edge",
            "catalog",
            "v1",
            "tenants",
            HARD_TENANT,
            "exports",
            "items-2026"
        ]
    );
    assert_eq!(echo, echo_of(HARD_TENANT));
    assert_eq!(export_id, "items-2026");

    // The export id is path data too: one segment, here an unknown export (404).
    let error = export(&tenant, "items/2026?all=1 #x").await.unwrap_err();
    let concord = error.concord().unwrap();
    assert_eq!(concord.kind(), ErrorKind::Status);
    assert_eq!(concord.status(), Some(StatusCode::NOT_FOUND));
    let seen = &server.requests()[1];
    assert_eq!(seen.segments()[6], "items/2026?all=1 #x");
    assert_eq!(seen.segments().len(), 7);
    Ok(())
}

#[tokio::test]
async fn x02_the_encoding_agrees_with_the_generated_endpoint() {
    let server = Server::start(catalog("/edge/catalog/v1")).await;
    let tenant = context(&server, "/edge/catalog/v1", "k-ext").tenant(HARD_TENANT);

    // Before execution: the SDK's generated endpoint and the outside operation.
    let generated = tenant.catalog_tenant().item("x").prepare().unwrap();
    let (_, built, _) = export_parts(&tenant, "items-2026").unwrap();
    assert_eq!(
        tenant_segment(generated.request().url().path()),
        tenant_segment(built.url().path())
    );

    // On the wire: one declared call, one outside call, the same tenant segment.
    tenant
        .catalog_tenant()
        .item("item-001")
        .send()
        .await
        .unwrap();
    export(&tenant, "items-2026").await.unwrap();
    let seen = server.requests();
    assert_eq!(seen.len(), 2);
    assert_eq!(tenant_segment(&seen[0].target), HARD_TENANT_WIRE);
    assert_eq!(tenant_segment(&seen[1].target), HARD_TENANT_WIRE);
    assert_eq!(seen[0].segments()[4], seen[1].segments()[4]);
}

#[tokio::test]
async fn x02_empty_and_dot_data_fail_before_http() {
    let server = Server::start(catalog("/catalog/v1")).await;
    let ctx = context(&server, "/catalog/v1", "k-ext");
    for bad in ["", ".", ".."] {
        let error = export(&ctx.tenant(bad), "items-2026").await.unwrap_err();
        assert_eq!(error.invalid_request(), Some(&ContextError::Tenant));
        assert!(error.concord().is_none());
        assert_eq!(error.operation(), EXPORT_OPERATION);

        let error = export(&ctx.tenant("acme"), bad).await.unwrap_err();
        assert_eq!(
            error.invalid_request(),
            Some(&ContextError::Segment { index: 1 })
        );
        assert!(export_parts(&ctx.tenant("acme"), bad).is_err());
    }
    let error = export(&ctx.tenant("acme"), "..").await.unwrap_err();
    assert_eq!(
        error.to_string(),
        "catalog-extension: tenant export: Catalog context: request not built: path segment 1 is empty, `.` or `..`"
    );
    assert_eq!(server.count(), 0);
    assert_eq!(server.connections(), 0);
}

// ---------------------------------------------------------------------------------------------
// X03: independent contexts; service and client-default headers on the wire.

#[tokio::test]
async fn x03_contexts_from_one_base_stay_independent() {
    let server = Server::start(catalog("/catalog/v1")).await;
    let mut defaults = HeaderMap::new();
    defaults.insert("x-app", HeaderValue::from_static("demo"));
    let http = reqwest::Client::builder()
        .default_headers(defaults)
        .build()
        .unwrap();
    let base = CatalogContext::new(
        http,
        CatalogConfig {
            key: "k-ext".into(),
        },
    )
    .with_base_url(format!("{}/catalog/v1", server.url()))
    .unwrap();

    let north_acme = base.clone().set_region("north").tenant("acme");
    let plain_globex = base.tenant("globex");
    // Deriving another context afterwards changes neither of them.
    let south_acme = north_acme
        .context()
        .clone()
        .set_region("south")
        .tenant("acme");

    // The client's default header is not on the built request: reqwest adds it when it
    // executes the request. Its absence here is not a loss; the wire below has it.
    let (_, built, _) = export_parts(&north_acme, "items-2026").unwrap();
    assert!(built.headers().get("x-app").is_none());
    assert_eq!(built.headers()["x-region"], "north");

    for tenant in [&north_acme, &plain_globex, &south_acme] {
        assert_eq!(export(tenant, "items-2026").await.unwrap(), rows());
    }
    // The generated handle of the same context sends the same context.
    north_acme
        .catalog_tenant()
        .item("item-001")
        .send()
        .await
        .unwrap();

    let seen = server.requests();
    assert_eq!(seen.len(), 4);
    let tenants: Vec<String> = seen.iter().map(|r| r.segments()[3].clone()).collect();
    assert_eq!(tenants, ["acme", "globex", "acme", "acme"]);
    let regions: Vec<Option<&str>> = seen.iter().map(|r| r.header("x-region")).collect();
    assert_eq!(regions, [Some("north"), None, Some("south"), Some("north")]);
    for request in &seen {
        assert_eq!(request.header("x-client"), Some("concord-example-sdk"));
        assert_eq!(request.header("x-api-key"), Some("k-ext"));
        assert_eq!(request.header("x-app"), Some("demo"));
    }
    assert_eq!(north_acme.context().region(), Some("north"));
    assert_eq!(plain_globex.context().region(), None);
}

// ---------------------------------------------------------------------------------------------
// X04: own representation, retained decoder, truthful identity.

#[tokio::test]
async fn x04_the_csv_decoder_is_kept_when_the_caller_changes_accept() {
    let server = Server::start(catalog("/catalog/v1")).await;
    let acme = context(&server, "/catalog/v1", "k-ext").tenant("acme");

    let rows_back = export_with(&acme, "items-2026", |request| {
        request
            .headers_mut()
            .insert(ACCEPT, HeaderValue::from_static("application/json"));
    })
    .await
    .unwrap();

    assert_eq!(rows_back, rows());
    assert_eq!(server.single().header_all("accept"), ["application/json"]);
}

#[tokio::test]
async fn x04_a_json_labelled_response_is_refused_unread_with_a_truthful_identity() {
    const TENANT: &str = "tenant-q7x";
    const KEY: &str = "key-q7x-s3cr3t";
    const JSON: &str = r#"[{"id":"item-001","name":"Notebook"}]"#;
    let server = Server::reply(Reply::json(200, JSON)).await;
    let tenant = context(&server, "/catalog/v1", KEY).tenant(TENANT);

    let error = export(&tenant, "items-2026").await.unwrap_err();

    assert_eq!(error.operation(), EXPORT_OPERATION);
    let concord = error.concord().unwrap();
    assert_eq!(concord.kind(), ErrorKind::Metadata);
    assert_eq!(concord.endpoint(), None);
    assert_eq!(
        concord.codec_error().unwrap().label(),
        "the response is not text/csv"
    );
    let shown = error.to_string();
    assert_eq!(
        shown,
        "catalog-extension: tenant export: response metadata not accepted: the response is not text/csv"
    );
    let debug = format!("{error:?} {error:#?}");
    for text in [&shown, &debug] {
        for leaked in [
            TENANT,
            KEY,
            "127.0.0.1",
            "http",
            "exports",
            "Catalog.tenant",
        ] {
            assert!(!text.contains(leaked), "{leaked} in {text}");
        }
    }

    // The response was not read: its body is still there.
    let response = error.into_concord().unwrap().into_response().unwrap();
    assert_eq!(response.bytes().await.unwrap(), JSON.as_bytes());
    assert_eq!(server.count(), 1);
}

#[tokio::test]
async fn x04_a_malformed_body_is_a_decode_error_with_positions_only() {
    let body = "id,name\nitem-001,Notebook\nitem-002,s3cr3t-cell,extra\n";
    let server = Server::reply(
        Reply::new(200)
            .header("content-type", "text/csv; charset=utf-8")
            .body(body),
    )
    .await;
    let acme = context(&server, "/catalog/v1", "k-ext").tenant("acme");

    let error = export(&acme, "items-2026").await.unwrap_err();

    let concord = error.concord().unwrap();
    assert_eq!(concord.kind(), ErrorKind::Decode);
    let codec = concord.codec_error().unwrap();
    assert_eq!(codec.label(), "a CSV row does not have 2 fields");
    assert_eq!(codec.context(), [("line", 3), ("fields", 3)]);
    let shown = format!("{error} {error:?}");
    assert!(!shown.contains("s3cr3t"), "{shown}");
}

fn head_check(content_type: &[&str]) -> Result<(), &'static str> {
    let mut headers = HeaderMap::new();
    for value in content_type {
        headers.append("content-type", HeaderValue::from_str(value).unwrap());
    }
    let head = ResponseHead::new(StatusCode::OK, Version::HTTP_11, &headers);
    <Csv as Decode<Vec<CsvRow>>>::check_content_type(&head).map_err(|e| e.label())
}

#[test]
fn x04_the_csv_format_checks_its_head_and_body() {
    assert_eq!(head_check(&["text/csv"]), Ok(()));
    assert_eq!(head_check(&["TEXT/CSV ; Charset=\"UTF-8\""]), Ok(()));
    assert_eq!(head_check(&["text/csv; header=present"]), Ok(()));
    assert_eq!(
        head_check(&["application/json"]),
        Err("the response is not text/csv")
    );
    assert_eq!(
        head_check(&["text/csv; charset=latin1"]),
        Err("the CSV charset is not utf-8")
    );
    assert_eq!(head_check(&[]), Err("the response has no Content-Type"));
    assert_eq!(
        head_check(&["text/csv", "text/csv"]),
        Err("the response repeats Content-Type")
    );
    assert_eq!(<Csv as Decode<Vec<CsvRow>>>::ACCEPT, Some("text/csv"));

    let decode = |body: &'static [u8]| {
        <Csv as Decode<Vec<CsvRow>>>::decode_body((), Bytes::from_static(body))
            .map_err(|e| (e.label(), e.context().to_vec()))
    };
    assert_eq!(
        decode(b"# note\r\nid,name\r\nitem-001,Notebook\r\n\r\n# trailer\n").unwrap(),
        rows()[..1]
    );
    assert_eq!(decode(b"id,name\n").unwrap(), []);
    assert_eq!(
        decode(b"name,id\n"),
        Err(("the CSV header is not `id,name`", vec![("line", 1)]))
    );
    assert_eq!(
        decode(b"id,name\nitem-001,\"Note\"\n"),
        Err((
            "quoted CSV fields are not supported",
            vec![("line", 2), ("column", 10)]
        ))
    );
    assert_eq!(
        decode(b"id,name\n\xffbad\n"),
        Err(("the CSV body is not UTF-8", vec![("byte", 8)]))
    );
    assert_eq!(
        decode(b"# only a comment\n"),
        Err(("the CSV body has no header line", vec![]))
    );
}

// ---------------------------------------------------------------------------------------------
// X05: native edit before one execution; transport failure.

#[tokio::test]
async fn x05_a_native_edit_adds_a_header_before_one_execution() {
    let server = Server::start(catalog("/catalog/v1")).await;
    let acme = context(&server, "/catalog/v1", "k-ext").tenant("acme");

    let rows_back = export_with(&acme, "items-2026", |request| {
        request
            .headers_mut()
            .insert("x-request-id", HeaderValue::from_static("req-42"));
        *request.timeout_mut() = Some(Duration::from_secs(5));
    })
    .await
    .unwrap();

    assert_eq!(rows_back, rows());
    let seen = server.single();
    assert_eq!(seen.header("x-request-id"), Some("req-42"));
    assert_eq!(seen.header("accept"), Some("text/csv"));
    assert_eq!(server.connections(), 1);
}

/// A loopback address nothing listens on: bound, then closed.
fn closed_port() -> std::net::SocketAddr {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);
    addr
}

#[tokio::test]
async fn x05_a_transport_failure_does_not_restore_the_url() {
    const SECRET: &str = "tenant-s3cr3t-77";
    let addr = closed_port();
    let http = reqwest::Client::builder().no_proxy().build().unwrap();
    let tenant: TenantContext = CatalogContext::new(
        http,
        CatalogConfig {
            key: "k-ext".into(),
        },
    )
    .with_base_url(format!("http://{addr}/catalog/v1"))
    .unwrap()
    .tenant(SECRET);

    // The documented boundary: the native request is not Concord output.
    let (_, built, _) = export_parts(&tenant, "items-2026").unwrap();
    assert!(format!("{built:?}").contains(SECRET));

    let error = export(&tenant, "items-2026").await.unwrap_err();

    let concord = error.concord().unwrap();
    assert_eq!(concord.kind(), ErrorKind::Transport);
    assert_eq!(concord.endpoint(), None);
    assert!(concord.transport().unwrap().is_connect());
    assert!(concord.transport().unwrap().url().is_none());
    assert_eq!(
        error.to_string(),
        "catalog-extension: tenant export: transport error (connect)"
    );
    let port = addr.port().to_string();
    let mut level: Option<&dyn std::error::Error> = Some(&error);
    let mut depth = 0;
    while let Some(current) = level {
        let shown = format!("{current} {current:?}");
        assert!(!shown.contains(SECRET), "level {depth}: {shown}");
        assert!(!shown.contains(&port), "level {depth}: {shown}");
        level = current.source();
        depth += 1;
    }
    assert_eq!(
        depth, 2,
        "the extension error, then Concord's (no further source)"
    );
}
