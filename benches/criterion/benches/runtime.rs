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
        // Dropping the input/output is excluded from the timing in every stage.
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

criterion_group!(benches, in_memory, roundtrip);
criterion_main!(benches);
