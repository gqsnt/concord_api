//! Criterion runtime suite: Concord V4 vs equivalent hand-written reqwest.
//!
//! `cargo bench -p concord-bench --bench runtime` (see `README.md` for IDs, baselines and
//! archiving). Benchmark IDs are `<group>/<scenario>/<implementation>/<stage>`; stages are
//! `input` (per-call input construction), `prepare` (request construction, including body
//! encoding), `decode`, `input_prepare` (both), and for round trips `send` (input + prepare +
//! execute + decode), `send_prepared` (execute + decode of a request prepared in setup) and
//! `get_bytes` (raw reqwest, no decoding).
#![allow(missing_docs)]

use std::hint::black_box;

use concord_baselines as base;
use concord_bench::payload::{self, Payload};
use concord_bench::{Case, Fixtures, GROUPS, ROUNDTRIP_LARGE, Runner, Throughput, apis};
use concord_bench::{http_client, run_group, server};
use concord_example_shapes as shapes;
use criterion::measurement::WallTime;
use criterion::{BatchSize, BenchmarkGroup, Criterion, criterion_group, criterion_main};

/// Adapts one Criterion benchmark group to the shared scenario definitions.
struct Group<'g, 'c>(&'g mut BenchmarkGroup<'c, WallTime>);

impl Runner for Group<'_, '_> {
    fn case<I, O>(
        &mut self,
        case: Case,
        mut input: impl FnMut() -> I,
        mut run: impl FnMut(I) -> O,
    ) {
        let g = &mut *self.0;
        match case.throughput {
            Some(Throughput::Elements(n)) => g.throughput(criterion::Throughput::Elements(n)),
            Some(Throughput::Bytes(n)) => g.throughput(criterion::Throughput::Bytes(n)),
            None => g,
        };
        // Inputs consumed by the routine are built in batches outside the timed region.
        // SmallInput keeps ~10% of the iterations' inputs alive at once; for >= 1 MB bodies
        // LargeInput bounds that memory (and the cache pollution it causes).
        let size = if case.large_input {
            BatchSize::LargeInput
        } else {
            BatchSize::SmallInput
        };
        // Output destruction is excluded. The consuming routine owns its input: destruction
        // of consumed fields (including JSON payloads and response heads) is timed there.
        if !case.trivial_input {
            g.bench_function(format!("{}/input", case.id), |b| {
                b.iter_with_large_drop(&mut input)
            });
        }
        g.bench_function(format!("{}/{}", case.id, case.run_stage), |b| {
            b.iter_batched(&mut input, |i| run(black_box(i)), size)
        });
        if case.total {
            g.bench_function(format!("{}/input_{}", case.id, case.run_stage), |b| {
                b.iter_with_large_drop(|| run(black_box(input())))
            });
        }
    }
}

/// `construct`, `headers`, `compose`, `body_encode`, `decode`: no I/O.
fn in_memory(c: &mut Criterion) {
    let fx = Fixtures::new(http_client());
    for name in GROUPS {
        let mut group = c.benchmark_group(name);
        run_group(name, &fx, &mut Group(&mut group));
        group.finish();
    }
}

/// `roundtrip`: keep-alive HTTP/1.1 over loopback, one current-thread runtime running both the
/// client and the server (as the former harness did). Runtime, server, clients and the pooled
/// connection are set up before measuring.
/// Inventory: every `roundtrip/{small,10KB}/manual/{send,send_prepared}` is a partial floor
/// against the corresponding Concord ID: prepared Accept and metadata validation differ.
/// `raw_reqwest/get_bytes` is a transport floor (no JSON decode). Tests compare successful
/// decoded fixture values only; no prepared-stage or general error-path equality is promised.
fn roundtrip(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
    let large = payload::json_bytes(ROUNDTRIP_LARGE.target);
    let addr = rt.block_on(server::serve(&large));
    let http = http_client();
    let base_url = format!("http://{addr}/v1");
    let s1 = shapes::s1_simple::SimpleApi::new(http.clone())
        .with_base_url(base_url.clone())
        .expect("valid base");
    let s1_manual = base::s1_simple::SimpleApi::with_client(
        http.clone(),
        url::Url::parse(&format!("{base_url}/")).expect("valid base"),
    );
    let bodies = apis::bodies::Bodies::new(http.clone())
        .with_base_url(base_url.clone())
        .expect("valid base");
    let bodies_manual = concord_bench::manual::Bodies::new(http.clone(), &base_url);
    let health_url = format!("{base_url}/health");
    let json_url = format!("{base_url}/json");
    // Open the pooled connection outside the measurement.
    rt.block_on(async { s1.health().send().await.expect("loopback") });

    let (s1, s1_manual, bodies, bodies_manual, http) =
        (&s1, &s1_manual, &bodies, &bodies_manual, &http);
    let (health_url, json_url) = (health_url.as_str(), json_url.as_str());
    let mut g = c.benchmark_group("roundtrip");

    g.throughput(criterion::Throughput::Bytes(server::SMALL_BODY.len() as u64));
    g.bench_function("small/concord/send", |b| {
        b.to_async(&rt)
            .iter(|| async move { s1.health().send().await.expect("ok") })
    });
    g.bench_function("small/concord/send_prepared", |b| {
        b.to_async(&rt).iter_batched(
            || s1.health().prepare().expect("valid"),
            |p| async move { p.send().await.expect("ok") },
            BatchSize::SmallInput,
        )
    });
    g.bench_function("small/manual/send", |b| {
        b.to_async(&rt)
            .iter(|| async move { s1_manual.health().await.expect("ok") })
    });
    g.bench_function("small/manual/send_prepared", |b| {
        b.to_async(&rt).iter_batched(
            || http.get(health_url).build().expect("valid"),
            |req| async move {
                let resp = http.execute(req).await.expect("ok");
                resp.error_for_status()
                    .expect("2xx")
                    .json::<base::s1_simple::Health>()
                    .await
                    .expect("json")
            },
            BatchSize::SmallInput,
        )
    });
    g.bench_function("small/raw_reqwest/get_bytes", |b| {
        b.to_async(&rt).iter(|| async move {
            let r = http.get(health_url).send().await.expect("ok");
            r.bytes().await.expect("body")
        })
    });

    let label = ROUNDTRIP_LARGE.label;
    g.throughput(criterion::Throughput::Bytes(large.len() as u64));
    g.bench_function(format!("{label}/concord/send"), |b| {
        b.to_async(&rt)
            .iter(|| async move { bodies.get_json().send().await.expect("ok") })
    });
    g.bench_function(format!("{label}/concord/send_prepared"), |b| {
        b.to_async(&rt).iter_batched(
            || bodies.get_json().prepare().expect("valid"),
            |p| async move { p.send().await.expect("ok") },
            BatchSize::SmallInput,
        )
    });
    g.bench_function(format!("{label}/manual/send"), |b| {
        b.to_async(&rt)
            .iter(|| async move { bodies_manual.get_json().await.expect("ok") })
    });
    g.bench_function(format!("{label}/manual/send_prepared"), |b| {
        b.to_async(&rt).iter_batched(
            || http.get(json_url).build().expect("valid"),
            |req| async move {
                let resp = http.execute(req).await.expect("ok");
                resp.error_for_status()
                    .expect("2xx")
                    .json::<Payload>()
                    .await
                    .expect("json")
            },
            BatchSize::SmallInput,
        )
    });
    g.bench_function(format!("{label}/raw_reqwest/get_bytes"), |b| {
        b.to_async(&rt).iter(|| async move {
            let r = http.get(json_url).send().await.expect("ok");
            r.bytes().await.expect("body")
        })
    });
    g.finish();
}

/// `plan_scaling` (H14): the run-time name plan of a native chain (`HeaderPlan::compose`, run by
/// `prepare()`) and the validator's check of the same chain (`NameCheck`), alone, at 8, 32, 128
/// and 512 contributions: distinct names, appends to one name, overrides of one name, and a
/// set/remove alternation. No request, value or classification is involved.
/// Inventory: `plan_scaling/{8,32,128,512}/{distinct,appends,overrides,removals}/{compose,validate}`
/// has different features: runtime plan production versus declaration-name validation. Setup
/// prebuilds names/shapes, and there is no equal-work comparator or ratio promise.
fn plan_scaling(c: &mut Criterion) {
    use concord_core::__private::{EndpointMeta, HeaderOp, HeaderPlan, HeaderShape, NameCheck};
    use concord_core::reqwest::header::HeaderName;

    static META: EndpointMeta = EndpointMeta {
        name: "Bench.plan",
        method: "GET",
        route: "",
        query_pairs: false,
        labels: 0,
        sensitive_labels: &[],
        segments: 0,
        sensitive_segments: &[],
        sensitive_query: &[],
        sensitive_content_type: false,
        sensitive_accept: false,
        accept_removed: false,
    };

    fn run<const N: usize>(g: &mut BenchmarkGroup<'_, WallTime>, names: &[HeaderName]) {
        let shapes = |op_at: &dyn Fn(usize) -> HeaderOp| -> [HeaderShape; N] {
            std::array::from_fn(|i| HeaderShape {
                op: op_at(i),
                label: "c",
            })
        };
        let first_set = |rest: HeaderOp| move |i: usize| if i == 0 { HeaderOp::Set } else { rest };
        let cases: [(&str, [&HeaderName; N], [HeaderShape; N]); 4] = [
            (
                "distinct",
                std::array::from_fn(|i| &names[i]),
                shapes(&|_| HeaderOp::Set),
            ),
            (
                "appends",
                std::array::from_fn(|_| &names[0]),
                shapes(&first_set(HeaderOp::Append)),
            ),
            (
                "overrides",
                std::array::from_fn(|_| &names[0]),
                shapes(&first_set(HeaderOp::Override)),
            ),
            (
                "removals",
                std::array::from_fn(|_| &names[0]),
                shapes(&|i| {
                    if i % 2 == 0 {
                        HeaderOp::Set
                    } else {
                        HeaderOp::Remove
                    }
                }),
            ),
        ];
        for (shape, refs, shapes) in cases {
            g.bench_function(format!("{N}/{shape}/compose"), |b| {
                b.iter(|| {
                    HeaderPlan::<N>::compose(
                        black_box(refs),
                        black_box(&shapes),
                        [true; N],
                        [false; N],
                        false,
                        &META,
                    )
                    .map(|p| p.live[N - 1])
                })
            });
            g.bench_function(format!("{N}/{shape}/validate"), |b| {
                b.iter(|| {
                    let mut check = NameCheck::new();
                    check.endpoint("Bench.plan", black_box(refs), black_box(&shapes), false);
                    check.finish().is_ok()
                })
            });
        }
    }

    let names: Vec<HeaderName> = (0..512)
        .map(|i| HeaderName::from_bytes(format!("x-h{i}").as_bytes()).expect("valid name"))
        .collect();
    let mut g = c.benchmark_group("plan_scaling");
    run::<8>(&mut g, &names);
    run::<32>(&mut g, &names);
    run::<128>(&mut g, &names);
    run::<512>(&mut g, &names);
    g.finish();
}

criterion_group!(benches, in_memory, roundtrip, plan_scaling);
criterion_main!(benches);
