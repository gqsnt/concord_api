//! generated client: two path inputs and three engine query inputs.
#[path = "../../concord-core/benches/support.rs"]
mod support;
use concord::{Json, Secret};
concord::api! {
    pub client Riot(key: Secret) {
        headers { "x-riot-token" = client.key; }
        read Json;
        scope account(route: tour::AccountRoute) {
            base "https://{route}.api.riotgames.com";
            path "riot/account/v1/accounts";
            GET by_riot_id(game_name: String, tag_line: String, start?: u32, count?: u32, queue?: u32) -> serde_json::Value {
                path "by-riot-id/{game_name}/{tag_line}";
                query { start; count; queue; }
            }
        }
    }
}
fn hand_record() -> reqwest::Request {
    let mut url =
        reqwest::Url::parse("https://europe.api.riotgames.com/riot/account/v1/accounts/").unwrap();
    url.path_segments_mut()
        .unwrap()
        .pop_if_empty()
        .extend(["by-riot-id", "Ada", "EUW"]);
    url.query_pairs_mut()
        .extend_pairs([("start", "0"), ("count", "100"), ("queue", "420")]);
    let mut request = reqwest::Request::new(reqwest::Method::GET, url);
    let mut key: reqwest::header::HeaderValue = "bench-key".parse().unwrap();
    key.set_sensitive(true);
    request.headers_mut().insert("x-riot-token", key);
    request
}
use reqwest::{Method, Request, Url};

fn hand() -> Request {
    let mut url = Url::parse("https://shapes.example.com/guarded/").unwrap();
    url.path_segments_mut()
        .unwrap()
        .pop_if_empty()
        .push("path-key");
    url.query_pairs_mut()
        .extend_pairs([("key", "query-key"), ("cursor", "next")]);
    Request::new(Method::GET, url)
}
fn main() {
    c12_14_plain_stream_materialization_uses_only_legacy_claim_allocations();
    let mut c = support::criterion();
    let record = Riot::new("bench-key");
    let get_record = || {
        record
            .account(tour::Europe)
            .by_riot_id("Ada", "EUW")
            .start(0)
            .count(100)
            .queue(420)
    };
    let ours = get_record().request().unwrap();
    let raw = hand_record();
    assert_eq!(ours.method(), raw.method());
    assert_eq!(ours.url(), raw.url());
    assert_eq!(ours.headers(), raw.headers());
    assert_eq!(
        ours.headers()["x-riot-token"].is_sensitive(),
        raw.headers()["x-riot-token"].is_sensitive()
    );
    println!("measure of record: local api! Riot, two path inputs, three query inputs, secret header and route host");
    let record_allocs = support::bench(
        &mut c,
        "get/build/concord",
        || (),
        |()| async { get_record().request().unwrap() },
    );
    let record_raw_allocs = support::bench(
        &mut c,
        "get/build/reqwest",
        || (),
        |()| async { hand_record() },
    );
    let client = tour_gen::Shapes::new();
    let get = || {
        client
            .guarded("path-key")
            .query_key("query-key")
            .cursor("next")
    };
    let ours = get().request().unwrap();
    let raw = hand();
    assert_eq!(ours.method(), raw.method());
    assert_eq!(ours.url(), raw.url());
    assert_eq!(ours.headers(), raw.headers());
    println!("Secondary row: tour-gen::Shapes::guarded (one path input, two query inputs; both key placements are Secret)");
    let concord_allocs = support::bench(
        &mut c,
        "guarded/build/concord",
        || (),
        |()| async { get().request().unwrap() },
    );
    let raw_allocs = support::bench(
        &mut c,
        "guarded/build/reqwest",
        || (),
        |()| async { hand() },
    );
    if let Ok(max) = std::env::var("CONCORD_GET_MAX_ALLOCATIONS") {
        assert!(
            record_allocs - record_raw_allocs <= max.parse::<usize>().unwrap(),
            "allocation regression: {} added",
            record_allocs - record_raw_allocs
        );
        assert!(
            concord_allocs - raw_allocs <= max.parse::<usize>().unwrap(),
            "allocation regression: {} added",
            concord_allocs - raw_allocs
        );
        let request = |template: bool| {
            use concord_core::__private::{Core, Draft, Options, Piece};
            let mut draft = Draft::new(Method::GET);
            draft.base(&[Piece::Lit("https://shape.test/")]);
            if template {
                draft.query("q", &[Piece::Lit("a"), Piece::Val("value", &"b")]);
            } else {
                draft.query("q", &[Piece::Val("value", &"ab")]);
            }
            draft
                .finish(&Core::<()>::default(), &Options::default(), "storage.get")
                .unwrap()
        };
        let direct = support::allocations("template/direct", || (), |()| request(false));
        let template = support::allocations("template/joined", || (), |()| request(true));
        // The extra allocation is the template's joined text, never a singleton Vec.
        assert!(
            template <= direct + 1,
            "scalar template storage allocated an extra collection"
        );
    }
    c.final_summary();
}


// Count both phases and their total so allocations cannot move outside the
// measurement window. Ordinary materialization allocates the source list,
// lock guards and claimed-source table; construction has its own budget.
fn c12_14_plain_stream_materialization_uses_only_legacy_claim_allocations() {
    struct Empty;
    impl futures_core::Stream for Empty {
        type Item = Result<concord::bytes::Bytes, std::io::Error>;
        fn poll_next(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<Self::Item>> {
            std::task::Poll::Ready(None)
        }
    }
    let count = support::allocations(
        "stream/materialize/concord",
        || concord::Body::stream(Empty),
        |body| body.into_stream().unwrap(),
    );
    assert!(count <= 3, "ordinary stream materialization allocated {count} times; legacy claim machinery needs 3");
    let built = support::allocations(
        "stream/build/concord",
        || (),
        |()| concord::Body::stream(Empty),
    );
    let complete = support::allocations(
        "stream/build-and-materialize/concord",
        || (),
        |()| concord::Body::stream(Empty).into_stream().unwrap(),
    );
    assert!(built <= 2, "ordinary stream construction allocated {built} times; legacy construction needs 2");
    assert!(complete <= 5, "ordinary stream construction and materialization allocated {complete} times; legacy total is 5");
}
