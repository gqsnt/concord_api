use bytes::Bytes;
use concord_core::advanced::{MultipartBody, StreamBody};
use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

fn bench(c: &mut Criterion) {
    let payload = Bytes::from_static(b"benchmark payload");
    c.bench_function("request_body/reusable_stream_recipe", |b| {
        b.iter(|| black_box(StreamBody::from_bytes(payload.clone())));
    });
    c.bench_function("request_body/direct_multipart_recipe", |b| {
        b.iter(|| black_box(MultipartBody::new().bytes("payload", payload.clone())));
    });
}

criterion_group!(benches, bench);
criterion_main!(benches);
