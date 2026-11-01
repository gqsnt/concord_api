//! Request-construction and loopback-execution timing: Concord V4 vs the hand-written baselines.
//!
//! Run with `cargo run --release -p concord-perf`. Each figure is the median of 9 runs after one
//! warm-up run; min and max are printed to show jitter.
#![allow(missing_docs)]

use std::hint::black_box;
use std::time::{Duration, Instant};

use concord_baselines as base;
use concord_example_shapes as v4;

const RUNS: usize = 9;

fn measure(name: &str, iters: u32, mut f: impl FnMut()) {
    let mut samples = Vec::with_capacity(RUNS);
    for run in 0..=RUNS {
        let start = Instant::now();
        for _ in 0..iters {
            f();
        }
        let per = start.elapsed() / iters;
        if run > 0 {
            samples.push(per);
        }
    }
    report(name, iters, samples);
}

fn report(name: &str, iters: u32, mut samples: Vec<Duration>) {
    samples.sort();
    println!(
        "{name:<48} median {:>9.2?}  min {:>9.2?}  max {:>9.2?}  ({RUNS} runs x {iters} iters)",
        samples[RUNS / 2],
        samples[0],
        samples[RUNS - 1]
    );
}

async fn measure_async<F, Fut>(name: &str, iters: u32, mut f: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = ()>,
{
    let mut samples = Vec::with_capacity(RUNS);
    for run in 0..=RUNS {
        let start = Instant::now();
        for _ in 0..iters {
            f().await;
        }
        let per = start.elapsed() / iters;
        if run > 0 {
            samples.push(per);
        }
    }
    report(name, iters, samples);
}

/// Minimal keep-alive HTTP/1.1 server answering every request with a fixed JSON body.
async fn serve() -> std::net::SocketAddr {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                let body = br#"{"status":"ok","id":"1","name":"n","healthy":true,"title":"t","items":[],"next_cursor":null}"#;
                let head = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n",
                    body.len()
                );
                let mut buf = vec![0u8; 8192];
                let mut pending = Vec::new();
                loop {
                    let n = match stream.read(&mut buf).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => n,
                    };
                    pending.extend_from_slice(&buf[..n]);
                    while let Some(pos) = pending.windows(4).position(|w| w == b"\r\n\r\n") {
                        pending.drain(..pos + 4);
                        if stream.write_all(head.as_bytes()).await.is_err()
                            || stream.write_all(body).await.is_err()
                        {
                            return;
                        }
                    }
                }
            });
        }
    });
    addr
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let http = reqwest::Client::new();
    println!("== request construction (no I/O)");

    let shop_v4 = v4::s3_query::ShopApi::new(http.clone(), "token");
    let shop_base = base::s3_query::ShopApi::new(
        http.clone(),
        url::Url::parse("https://api.example.com/v1/").unwrap(),
        "token",
    )
    .unwrap();
    measure("S3 search, 9 params  concord prepare()", 200_000, || {
        let p = shop_v4
            .search()
            .set_q("red shoe")
            .set_tags(vec!["a".to_owned(), "b".to_owned()])
            .set_category("c")
            .set_min_price(1)
            .set_max_price(9)
            .set_in_stock(true)
            .set_sort(v4::s3_query::Sort::PriceAsc)
            .set_cursor("cur")
            .set_limit(50)
            .prepare()
            .unwrap();
        let _ = black_box(p);
    });
    measure("S3 search, 9 params  hand-written build()", 200_000, || {
        let r = shop_base
            .search()
            .q("red shoe")
            .tag("a")
            .tag("b")
            .category("c")
            .min_price(1)
            .max_price(9)
            .in_stock(true)
            .sort(base::s3_query::Sort::PriceAsc)
            .cursor(Some("cur".to_owned()))
            .limit(50)
            .into_request()
            .build()
            .unwrap();
        black_box(r);
    });

    let cat_v4 = v4::s2_scoped::CatalogApi::new(http.clone(), "key");
    let cat_base = base::s2_scoped::CatalogApi::with_client(
        http.clone(),
        "https://api.example.com/v2/",
        "key",
    )
    .unwrap();
    measure(
        "S2 product (scopes+subdomain) concord prepare()",
        200_000,
        || {
            let p = cat_v4
                .regional(v4::s2_scoped::Region::Eu)
                .org("acme")
                .product("p-1")
                .prepare()
                .unwrap();
            let _ = black_box(p);
        },
    );
    measure(
        "S2 product (scopes+subdomain) hand-written build()",
        200_000,
        || {
            let r = cat_base
                .request(
                    reqwest::Method::GET,
                    base::s2_scoped::Region::Eu,
                    &["orgs", "acme", "catalog", "products", "p-1"],
                )
                .build()
                .unwrap();
            black_box(r);
        },
    );

    println!("== loopback execution (same reqwest::Client, same server, keep-alive)");
    let addr = serve().await;
    let simple_v4 = v4::s1_simple::SimpleApi::new(http.clone())
        .with_base_url(format!("http://{addr}/v1"))
        .unwrap();
    let simple_base = base::s1_simple::SimpleApi::with_client(
        http.clone(),
        url::Url::parse(&format!("http://{addr}/v1/")).unwrap(),
    );
    let raw_url = format!("http://{addr}/v1/health");
    measure_async(
        "S1 health  raw reqwest get().send().bytes()",
        5_000,
        || async {
            let r = http.get(&raw_url).send().await.unwrap();
            black_box(r.bytes().await.unwrap());
        },
    )
    .await;
    measure_async("S1 health  hand-written send() (json)", 5_000, || async {
        black_box(simple_base.health().await.unwrap());
    })
    .await;
    measure_async("S1 health  concord send() (json)", 5_000, || async {
        black_box(simple_v4.health().send().await.unwrap());
    })
    .await;
}
