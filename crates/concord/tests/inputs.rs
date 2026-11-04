//! Generated input records (experiment G1, `v4/dx-4.3/DECISIONS.md`): `{Endpoint}Input`, the
//! request's `input`/`input_mut`/`into_input`, and the parent's `{endpoint}_with`.
//!
//! Requirements I01–I08 and C03 of the V4.3 candidate. Every assertion compares the input form
//! with the setter form of the same request, or observes the wire; counters are thread-local
//! because each test runs on its own thread.
//!
//! The declaration uses only formats defined here, so the file compiles under every feature set
//! the maintained gate tests (`--no-default-features` included).

mod support;

use std::cell::Cell;
use std::fmt;
use std::sync::{Arc, Mutex};

use concord::bytes::Bytes;
use concord::reqwest;
use concord::{CodecError, Encode};
use support::{Reply, Server};

thread_local! {
    static LIMIT_INITS: Cell<usize> = const { Cell::new(0) };
    static DISPLAYS: Cell<usize> = const { Cell::new(0) };
    static ENCODES: Cell<usize> = const { Cell::new(0) };
}

fn bump(counter: &'static std::thread::LocalKey<Cell<usize>>) {
    counter.with(|c| c.set(c.get() + 1));
}

fn count(counter: &'static std::thread::LocalKey<Cell<usize>>) -> usize {
    counter.with(Cell::get)
}

/// The initializer of `limit`, instrumented.
fn default_limit() -> u32 {
    bump(&LIMIT_INITS);
    20
}

/// A filter value whose `Display` is counted and whose text is shared between clones (an `Arc`
/// inside: a clone of an input is a shallow structural copy).
#[derive(Clone)]
pub struct Shared(Arc<Mutex<String>>);

impl Shared {
    fn new(text: &str) -> Self {
        Self(Arc::new(Mutex::new(text.to_owned())))
    }
}

impl fmt::Display for Shared {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        bump(&DISPLAYS);
        f.write_str(&self.0.lock().unwrap())
    }
}

/// A one-shot, deliberately non-`Clone` body.
pub struct OneShot(Bytes);

/// Its format: moves the bytes into the body, counting encodings.
pub struct Raw;

impl Encode<OneShot> for Raw {
    const CONTENT_TYPE: Option<&'static str> = Some("application/octet-stream");

    fn encode(value: OneShot) -> Result<reqwest::Body, CodecError> {
        bump(&ENCODES);
        Ok(reqwest::Body::from(value.0))
    }
}

/// Plain text bodies.
pub struct Plain;

impl Encode<String> for Plain {
    const CONTENT_TYPE: Option<&'static str> = Some("text/plain");

    fn encode(value: String) -> Result<reqwest::Body, CodecError> {
        Ok(reqwest::Body::from(value))
    }
}

concord::api! {
    pub client Shop(
        #[sensitive] header("x-api-key") key: String,
        header("x-region") region?: String = "north",
    ) {
        base "http://shop.test/v1";

        scope tenant(tenant: String) {
            path "tenants/{tenant}";

            scope listing(query("tag") each default_tags: Vec<String> = Vec::new()) {
                GET list(
                    query q?: Shared,
                    tags?: Vec<String>,
                    query limit: u32 = default_limit(),
                    region?: Option<String>,
                    #[sensitive] query token?: String,
                    query cursor?: String,
                ) {
                    path "items";
                    query { override "tag" = each tags else inherit; }
                    headers { override "x-region" = each region else inherit; }
                }
            }

            GET item(id: String) { path "items/{id}"; }

            PUT upload(id: String, body file: OneShot) {
                path "items/{id}/file";
                request Raw;
            }

            POST note(body text?: String) {
                path "notes";
                request Plain;
            }

            #[deprecated(note = "use `item`")]
            GET old() { path "old"; }

            // Never compiled: its input record is gated with it, or `NotEvenAType` would fail.
            #[cfg(any())]
            GET never(x: NotEvenAType) { path "never/{x}"; }
        }
    }
}

fn shop() -> Shop {
    Shop::new(reqwest::Client::new(), ShopConfig { key: "k-1".into() })
}

fn featured(shop: &Shop) -> ShopTenantListing {
    shop.tenant("acme")
        .listing()
        .set_default_tags(vec!["featured".to_owned()])
}

/// The query string and `x-region` values a request would send.
fn wire(request: ShopTenantListingList) -> (Option<String>, Vec<String>) {
    let prepared = request.prepare().unwrap();
    let query = prepared.request().url().query().map(str::to_owned);
    let region = prepared
        .request()
        .headers()
        .get_all("x-region")
        .iter()
        .map(|v| v.to_str().unwrap().to_owned())
        .collect();
    (query, region)
}

/// Compile-time: `NotClone::<_>::check` resolves only for a type that is NOT `Clone` (for a
/// `Clone` type both impls apply and the call is ambiguous).
trait NotClone<A> {
    fn check() {}
}
impl<T: ?Sized> NotClone<()> for T {}
struct IfClone;
impl<T: Clone> NotClone<IfClone> for T {}

fn is_clone<T: Clone>() {}

#[test]
fn i01_the_macro_generates_inputs_with_the_documented_shape() {
    // Types and signatures, checked by the compiler.
    let _: fn() -> ShopTenantListingListInput = ShopTenantListingListInput::new;
    let _: fn(&ShopTenantListing, ShopTenantListingListInput) -> ShopTenantListingList =
        ShopTenantListing::list_with;
    let _: fn(&ShopTenantListingList) -> &ShopTenantListingListInput = ShopTenantListingList::input;
    let _: fn(ShopTenantListingList) -> ShopTenantListingListInput =
        ShopTenantListingList::into_input;
    let _: fn(&ShopTenant, ShopTenantUploadInput) -> ShopTenantUpload = ShopTenant::upload_with;

    let input = ShopTenantListingListInput::new();
    let _: &Option<Shared> = &input.q;
    let _: &Option<Vec<String>> = &input.tags;
    let _: &u32 = &input.limit;
    let _: &Option<Option<String>> = &input.region;
    let _: &Option<String> = &input.token;

    // Required values are `new`'s arguments, as for the navigation method.
    let item = ShopTenantItemInput::new("item-7");
    assert_eq!(item.id, "item-7");
    let upload = ShopTenantUploadInput::new("a", OneShot(Bytes::from_static(b"x")));
    assert_eq!(upload.id, "a");
    let note = ShopTenantNoteInput::new();
    assert!(note.text.is_none());

    // Conditional `Clone`, exactly as for the request types.
    is_clone::<ShopTenantListingListInput>();
    is_clone::<ShopTenantListingList>();
    <ShopTenantUploadInput as NotClone<_>>::check();
    <ShopTenantUpload as NotClone<_>>::check();

    // A `#[deprecated]` endpoint deprecates both navigation methods; its input is not deprecated.
    #[allow(deprecated)]
    let old = shop().tenant("acme").old_with(ShopTenantOldInput::new());
    assert_eq!(
        old.prepare().unwrap().request().url().path(),
        "/v1/tenants/acme/old"
    );
}

#[test]
fn i03_absent_inherit_empty_and_replacement_send_what_the_setters_send() {
    let shop = shop();
    let listing = featured(&shop);
    let input = |edit: fn(&mut ShopTenantListingListInput)| {
        let mut input = ShopTenantListingListInput::new();
        edit(&mut input);
        listing.list_with(input)
    };

    // Absent: inherited tags and the client's region.
    let inherit = wire(input(|_| {}));
    assert_eq!(inherit, wire(listing.list()));
    assert_eq!(
        inherit,
        (Some("tag=featured&limit=20".into()), vec!["north".into()])
    );

    // Explicit empty replacement: no `tag` at all, the key's position unchanged otherwise.
    let empty = wire(input(|i| i.tags = Some(Vec::new())));
    assert_eq!(empty, wire(listing.list().set_tags(Vec::new())));
    assert_eq!(empty.0.as_deref(), Some("limit=20"));

    // Replacement keeps the inherited key's position and the items' order.
    let replaced = wire(input(|i| i.tags = Some(vec!["b".into(), "a".into()])));
    assert_eq!(
        replaced,
        wire(
            listing
                .list()
                .set_tags(vec!["b".to_owned(), "a".to_owned()])
        )
    );
    assert_eq!(replaced.0.as_deref(), Some("tag=b&tag=a&limit=20"));

    // `region: Option<Option<String>>`: inherit, omit, replace.
    let omit = wire(input(|i| i.region = Some(None)));
    assert_eq!(omit, wire(listing.list().set_region(None)));
    assert!(omit.1.is_empty());
    let south = wire(input(|i| i.region = Some(Some("south".into()))));
    assert_eq!(
        south,
        wire(listing.list().set_region(Some("south".to_owned())))
    );
    assert_eq!(south.1, ["south"]);

    // `None` is never force-omit: a `None` tags field after `Some` brings the inherited tags back.
    let mut back = listing.list().set_tags(Vec::new());
    back.input_mut().tags = None;
    assert_eq!(wire(back), inherit);

    // An optional value without an override: absent sends nothing; optional order is the
    // declared one.
    let mut all = ShopTenantListingListInput::new();
    all.cursor = Some("c2".into());
    all.q = Some(Shared::new("lamp"));
    assert_eq!(
        wire(listing.list_with(all)).0.as_deref(),
        Some("tag=featured&q=lamp&limit=20&cursor=c2")
    );
}

#[test]
fn i04_initializers_run_once_per_creation_never_on_inspection() {
    let shop = shop();
    let listing = featured(&shop);
    let start = count(&LIMIT_INITS);

    let input = ShopTenantListingListInput::new();
    assert_eq!(
        count(&LIMIT_INITS),
        start + 1,
        "new evaluates the initializer"
    );

    let copy = input.clone();
    let _ = format!("{input:?}");
    let request = listing.list_with(copy);
    let _ = request.input().limit;
    let request = request.set_q(Shared::new("x"));
    let again = request.clone().into_input();
    let rebound = listing.list_with(again);
    let _ = rebound.prepare().unwrap();
    assert_eq!(
        count(&LIMIT_INITS),
        start + 1,
        "clone, Debug, input(), _with, into_input and prepare do not"
    );

    let _ = listing.list();
    assert_eq!(
        count(&LIMIT_INITS),
        start + 2,
        "the navigation method does, once"
    );

    // The initialized scalar has no absent state: omitting `limit` is not expressible, only
    // replacing it.
    let mut five = ShopTenantListingListInput::new();
    five.limit = 5;
    assert_eq!(
        wire(listing.list_with(five)).0.as_deref(),
        Some("tag=featured&limit=5")
    );
}

#[test]
fn i07_inspection_formats_nothing_prepare_formats_each_placement() {
    let shop = shop();
    let listing = featured(&shop);
    let mut input = ShopTenantListingListInput::new();
    input.q = Some(Shared::new("lamp"));
    let start = count(&DISPLAYS);

    let copy = input.clone();
    let request = listing.list_with(copy);
    let _ = request.input().q.is_some();
    let request = listing.list_with(request.into_input());
    assert_eq!(
        count(&DISPLAYS),
        start,
        "reading, cloning and rebinding format nothing"
    );

    let _ = format!("{input:?}");
    assert_eq!(
        count(&DISPLAYS),
        start + 1,
        "Debug formats explicitly, once per value"
    );

    let _ = request.prepare().unwrap();
    assert_eq!(
        count(&DISPLAYS),
        start + 2,
        "prepare formats the one placement"
    );
}

#[test]
fn i06_a_clone_is_shallow_and_a_move_only_body_is_moved_once() {
    // Shallow: an `Arc` inside a value stays shared between clones.
    let mut input = ShopTenantListingListInput::new();
    input.q = Some(Shared::new("lamp"));
    let copy = input.clone();
    *input.q.as_ref().unwrap().0.lock().unwrap() = "desk".to_owned();
    assert_eq!(*copy.q.as_ref().unwrap().0.lock().unwrap(), "desk");

    // A move-only body: borrowed by inspection without encoding, moved by `into_input`/`_with`.
    let shop = shop();
    let tenant = shop.tenant("acme");
    let start = count(&ENCODES);
    let request = tenant.upload("a", OneShot(Bytes::from_static(b"payload")));
    assert_eq!(request.input().file.0.len(), 7);
    let _ = format!("{request:?} {:?}", request.input());
    let input = request.into_input();
    let request = tenant.upload_with(input);
    assert_eq!(count(&ENCODES), start, "inspection does not encode");
    let prepared = request.prepare().unwrap();
    assert_eq!(count(&ENCODES), start + 1, "prepare encodes once");
    assert_eq!(
        prepared.request().body().and_then(|b| b.as_bytes()),
        Some(&b"payload"[..])
    );
}

#[tokio::test]
async fn i05_store_branch_rebind_and_keep_the_ordinary_sibling() {
    let server = Server::start(|_| Reply::new(204)).await;
    let shop = shop()
        .with_base_url(format!("{}/v1", server.url()))
        .unwrap();
    let acme = featured(&shop);
    let studio = shop
        .tenant("studio")
        .listing()
        .set_default_tags(vec!["featured".to_owned()]);

    // A stored search, independent of any tenant.
    let mut saved = ShopTenantListingListInput::new();
    saved.q = Some(Shared::new("lamp"));
    saved.limit = 5;

    // Branch it: one call without the inherited tags, one with its own.
    let mut unfiltered = saved.clone();
    unfiltered.tags = Some(Vec::new());
    let mut office = saved.clone();
    office.tags = Some(vec!["office".to_owned()]);

    acme.list_with(saved.clone()).send().await.unwrap();
    studio.list_with(saved.clone()).send().await.unwrap();
    acme.list_with(unfiltered).send().await.unwrap();
    studio.list_with(office).send().await.unwrap();
    acme.list().send().await.unwrap(); // the ordinary sibling

    let seen: Vec<String> = server.requests().iter().map(|r| r.target.clone()).collect();
    assert_eq!(
        seen,
        [
            "/v1/tenants/acme/items?tag=featured&q=lamp&limit=5",
            "/v1/tenants/studio/items?tag=featured&q=lamp&limit=5",
            "/v1/tenants/acme/items?q=lamp&limit=5",
            "/v1/tenants/studio/items?tag=office&q=lamp&limit=5",
            "/v1/tenants/acme/items?tag=featured&limit=20",
        ]
    );
    // The stored input is unchanged by its branches and by being sent.
    assert!(saved.tags.is_none());
    assert_eq!(saved.limit, 5);

    // Context is outside the input: the region comes from the handle it is bound to.
    let south = shop.clone().set_region("south");
    let bound = south
        .tenant("acme")
        .listing()
        .list_with(saved.clone())
        .prepare()
        .unwrap();
    assert_eq!(bound.request().headers()["x-region"], "south");
}

#[test]
fn i08_debug_matches_the_request_and_withholds_sensitive_values() {
    let shop = shop();
    let listing = featured(&shop);
    let request = listing
        .list()
        .set_token("SYNTHETIC-TOKEN")
        .set_q(Shared::new("lamp"));
    let shown = format!("{:?}", request.input());
    assert!(!shown.contains("SYNTHETIC-TOKEN"), "{shown}");
    assert!(shown.contains("token: Some(<redacted>)"), "{shown}");
    assert_eq!(
        shown,
        format!("{request:?}").replacen("ShopTenantListingList", "ShopTenantListingListInput", 1)
    );
    // The client's key is not part of any input.
    assert!(!shown.contains("k-1"));
    // A body is shown as `..`, never encoded or read.
    let upload = shop
        .tenant("acme")
        .upload("a", OneShot(Bytes::from_static(b"SECRET-BODY")));
    let shown = format!("{:?}", upload.input());
    assert_eq!(shown, r#"ShopTenantUploadInput { id: "a", file: ".." }"#);
}

#[test]
fn input_mut_is_the_setters_in_place() {
    let shop = shop();
    let listing = featured(&shop);
    let mut edited = listing.list();
    edited.input_mut().limit = 7;
    edited.input_mut().cursor = Some("c3".into());
    let set = listing.list().set_limit(7).set_cursor("c3");
    assert_eq!(wire(edited), wire(set));

    // An optional body: `None` sends none, `Some` sends it.
    let tenant = shop.tenant("acme");
    let mut note = tenant.note();
    assert!(note.clone().prepare().unwrap().request().body().is_none());
    note.input_mut().text = Some("hello".into());
    assert_eq!(
        note.prepare()
            .unwrap()
            .request()
            .body()
            .and_then(|b| b.as_bytes()),
        Some(&b"hello"[..])
    );
}
