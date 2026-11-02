//! The code of `docs/CALLER_GUIDE.md`, one test per section. The guide quotes these lines
//! verbatim; `tests/doc_excerpts.rs` fails if a quote and this file drift apart.

use std::time::Duration;

use concord::{Decode, ErrorKind, Json, reqwest};
use concord_example_quickstart::local_server::{LocalServer, Response};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// A product.
#[derive(Debug, Clone, Deserialize)]
pub struct Product {
    /// Identifier.
    pub id: u64,
    /// Name.
    pub name: String,
}

/// The body of `PUT .../products/{id}`.
#[derive(Debug, Clone, Serialize)]
pub struct Rename {
    /// The new name.
    pub name: String,
}

/// The API's error document.
#[derive(Debug, Deserialize)]
pub struct Problem {
    /// A short summary.
    pub title: String,
}

concord::api! {
    /// A product catalog.
    pub client Catalog(
        /// The API key.
        #[sensitive] header("x-api-key") key: String,
        /// The preferred language of product names.
        header("accept-language") lang?: String = "en",
    ) {
        base "https://catalog.example.com/v1";

        /// One shop.
        scope shop(shop: String) {
            path "shops/{shop}";

            /// Lists products.
            GET products(
                query category?: String,
                query limit: u32 = 20,
                language?: Option<String>,
            ) -> Vec<Product> via Json {
                path "products";
                headers { override "accept-language" = each language else inherit; }
            }

            /// One product.
            GET product(id: u64) -> Product via Json { path "products/{id}"; }

            /// Renames a product.
            PUT rename(id: u64, body change: Rename via Json) -> Product via Json {
                path "products/{id}";
            }

            /// Deletes a product.
            DELETE delete(id: u64) { path "products/{id}"; }
        }
    }
}

/// The `accept-language` header a request would send.
fn language_of(request: CatalogShopProducts) -> Result<Option<String>, concord::Error> {
    let prepared = request.prepare()?;
    let header = prepared.request().headers().get("accept-language");
    Ok(header.map(|value| value.to_str().unwrap().to_owned()))
}

/// The query string a request would send.
fn query_of(request: CatalogShopProducts) -> Result<Option<String>, concord::Error> {
    let prepared = request.prepare()?;
    Ok(prepared.request().url().query().map(str::to_owned))
}

/// A client whose base URL is the local stand-in server.
fn local(server: &LocalServer) -> Result<Catalog, concord::Error> {
    let api = Catalog::new(reqwest::Client::new(), CatalogConfig { key: "k1".into() });
    api.with_base_url(format!("{}/v1", server.url()))
}

#[test]
fn creating_a_client() -> Result<(), Box<dyn std::error::Error>> {
    // Transport settings live on your reqwest::Client.
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;
    let api = Catalog::new(http, CatalogConfig { key: "k1".into() });

    // The same declaration against another deployment (staging, a local test server):
    let staging = api
        .clone()
        .with_base_url("https://staging.catalog.example.com/v1")?;
    let request = staging.shop("acme").product(7).prepare()?;
    assert_eq!(
        request.request().url().as_str(),
        "https://staging.catalog.example.com/v1/shops/acme/products/7"
    );

    // Concord's Debug output withholds sensitive values.
    assert_eq!(
        format!("{api:?}"),
        r#"Catalog { base: "https://catalog.example.com/v1", key: <redacted>, lang: Some("en") }"#
    );
    Ok(())
}

#[test]
fn handles_are_snapshots() -> Result<(), concord::Error> {
    let api = Catalog::new(reqwest::Client::new(), CatalogConfig { key: "k1".into() });
    let acme = api.shop("acme"); // borrows `api`, keeps a snapshot of it
    let request = acme.products(); // a request value; nothing is sent yet

    let german = api.clone().set_lang("de"); // a new handle; `api` is unchanged
    assert_eq!(
        language_of(german.shop("acme").products())?.as_deref(),
        Some("de")
    );

    // Handles and requests created earlier keep the values they captured.
    assert_eq!(language_of(request)?.as_deref(), Some("en"));
    assert_eq!(language_of(acme.products())?.as_deref(), Some("en"));
    Ok(())
}

#[test]
fn setting_and_omitting_values() -> Result<(), concord::Error> {
    let api = Catalog::new(reqwest::Client::new(), CatalogConfig { key: "k1".into() });
    let acme = api.shop("acme");

    let lamps = acme.products().set_category("lamps").set_limit(50);
    assert_eq!(
        query_of(lamps.clone())?.as_deref(),
        Some("category=lamps&limit=50")
    );

    // `maybe_x` sets exactly the given presence: pass an `Option` you already have.
    let category: Option<String> = None;
    let all = lamps.maybe_category(category);
    assert_eq!(query_of(all)?.as_deref(), Some("limit=50"));

    // `language?: Option<String>`: inherit, omit or replace, chosen per call.
    let inherit = acme.products();
    let omit = acme.products().set_language(None);
    let replace = acme.products().set_language(Some("fr".to_owned()));
    assert_eq!(language_of(inherit)?.as_deref(), Some("en"));
    assert_eq!(language_of(omit)?, None);
    assert_eq!(language_of(replace)?.as_deref(), Some("fr"));

    // On an initialized optional value, `maybe_x(None)` means absent, not "the initial value".
    let silent = api.clone().maybe_lang(None);
    assert_eq!(language_of(silent.shop("acme").products())?, None);
    Ok(())
}

#[tokio::test]
async fn preparing_and_sending() -> Result<(), Box<dyn std::error::Error>> {
    let server =
        LocalServer::start(|_| Response::json(200, json!([{ "id": 7, "name": "Lamp" }]))).await?;
    let api = local(&server)?;
    let request = api.shop("acme").products().set_category("lamps");

    // prepare(): builds the native request; no I/O.
    let prepared = request.clone().prepare()?;
    assert_eq!(prepared.request().method(), reqwest::Method::GET);
    let shown = format!("{prepared:?}"); // redacted: `x-api-key: <redacted>`
    assert!(shown.contains("x-api-key: <redacted>") && !shown.contains("k1"));

    // send(): prepare, execute once, require a 2xx status, decode as declared.
    let products: Vec<Product> = request.send().await?;
    assert_eq!(products[0].name, "Lamp");

    // An endpoint without `->` returns `()`; any 2xx body is read and dropped.
    api.shop("acme").delete(7).send().await?;

    // send_raw(): the native response, status unchecked and body unread.
    let response = api.shop("acme").product(7).prepare()?.send_raw().await?;
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    Ok(())
}

/// `Ok(None)` when the product does not exist; every other failure is an error.
async fn find_product(shop: &CatalogShop, id: u64) -> Result<Option<Product>, concord::Error> {
    match shop.product(id).send().await {
        Ok(product) => Ok(Some(product)),
        Err(error) if error.status() == Some(reqwest::StatusCode::NOT_FOUND) => Ok(None),
        Err(error) => Err(error),
    }
}

#[tokio::test]
async fn handling_errors() -> Result<(), Box<dyn std::error::Error>> {
    let server = LocalServer::start(|_| Response::problem(404, "no such product")).await?;
    let api = local(&server)?;
    assert!(find_product(&api.shop("acme"), 99).await?.is_none());

    let error = api.shop("acme").product(99).send().await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Status);
    assert_eq!(error.status(), Some(reqwest::StatusCode::NOT_FOUND));
    assert_eq!(error.endpoint(), Some("Catalog.shop.product"));
    assert_eq!(
        error.to_string(),
        "GET Catalog.shop.product: HTTP status 404 Not Found"
    );

    // The API's error document: the response of a status error is unread.
    let response = error.into_response()?; // any other kind of error is returned as Err
    let problem = <Json as Decode<Problem>>::decode(response.bytes().await?)?;
    assert_eq!(problem.title, "no such product");

    // A value that cannot form a request is a build error; nothing is sent.
    let error = api.shop("..").product(1).prepare().unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Build);
    assert_eq!(
        error.to_string(),
        "GET Catalog.shop.product: request not built: path segment `{shop}` is empty, `.` or `..`"
    );
    Ok(())
}

#[tokio::test]
async fn native_requests() -> Result<(), Box<dyn std::error::Error>> {
    let server =
        LocalServer::start(|_| Response::json(200, json!({ "id": 7, "name": "Lamp" }))).await?;
    let api = local(&server)?;

    let prepared = api.shop("acme").product(7).prepare()?;
    let (client, mut request, decoder) = prepared.into_parts();
    *request.timeout_mut() = Some(Duration::from_secs(2));
    let request_id = reqwest::header::HeaderValue::from_static("req-1");
    request.headers_mut().insert("x-request-id", request_id);
    let product: Product = decoder.finish(client.execute(request).await).await?;

    assert_eq!(product.id, 7);
    assert_eq!(server.requests()[0].header("x-request-id"), Some("req-1"));
    Ok(())
}

#[test]
fn handles_and_requests_are_send_and_sync() {
    fn shareable<T: Send + Sync + Clone>() {}
    shareable::<Catalog>();
    shareable::<CatalogShop>();
    shareable::<CatalogShopRename>();
}

/// The generated signatures listed in the guide, checked by coercion to function pointers
/// (arguments written as `impl IntoValue<T>` are instantiated with `T`).
#[test]
fn the_documented_signatures() {
    use concord::{Error, Prepared};
    type Products = CatalogShopProducts;

    let _: fn(reqwest::Client, CatalogConfig) -> Catalog = Catalog::new;
    let _: fn(Catalog, String) -> Result<Catalog, Error> = Catalog::with_base_url;
    let _: fn(Catalog, String) -> Catalog = Catalog::set_key;
    let _: fn(Catalog, String) -> Catalog = Catalog::set_lang;
    let _: fn(Catalog, Option<String>) -> Catalog = Catalog::maybe_lang;
    let _: fn(&Catalog, String) -> CatalogShop = Catalog::shop;
    let _: fn(CatalogShop, String) -> CatalogShop = CatalogShop::set_shop;
    let _: fn(&CatalogShop) -> Products = CatalogShop::products;
    let _: fn(&CatalogShop, u64) -> CatalogShopProduct = CatalogShop::product;
    let _: fn(&CatalogShop, u64, Rename) -> CatalogShopRename = CatalogShop::rename;
    let _: fn(&CatalogShop, u64) -> CatalogShopDelete = CatalogShop::delete;
    let _: fn(Products, String) -> Products = Products::set_category;
    let _: fn(Products, Option<String>) -> Products = Products::maybe_category;
    let _: fn(Products, u32) -> Products = Products::set_limit;
    let _: fn(Products, Option<String>) -> Products = Products::set_language;
    let _: fn(Products, Option<Option<String>>) -> Products = Products::maybe_language;
    let _: fn(CatalogShopRename, Rename) -> CatalogShopRename = CatalogShopRename::set_change;
    let _: fn(Products) -> Result<Prepared<Vec<Product>>, Error> = Products::prepare;
    let _: fn(CatalogShopDelete) -> Result<Prepared<()>, Error> = CatalogShopDelete::prepare;
}
