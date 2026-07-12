use concord_core::advanced::{DynBody, StreamBody};
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use perf::support::{
    ChunkedBodyStream, InMemoryAsyncRead, drain_transport_stream, filled_bytes, runtime,
};
use std::env;
use std::hint::black_box;

const ONE_MIB: usize = 1024 * 1024;
const SIXTEEN_MIB: usize = 16 * ONE_MIB;
const ONE_HUNDRED_TWENTY_EIGHT_MIB: usize = 128 * ONE_MIB;

fn full_fixture_enabled() -> bool {
    matches!(env::var("CONCORD_PERF_FULL"), Ok(value) if value == "1")
}

fn payload_sizes() -> Vec<usize> {
    let mut sizes = vec![ONE_MIB, SIXTEEN_MIB];
    if full_fixture_enabled() {
        sizes.push(ONE_HUNDRED_TWENTY_EIGHT_MIB);
    }
    sizes
}

fn chunk_sizes() -> &'static [usize] {
    &[1024, 8 * 1024, 64 * 1024]
}

fn size_label(size: usize) -> &'static str {
    match size {
        ONE_MIB => "1MiB",
        SIXTEEN_MIB => "16MiB",
        ONE_HUNDRED_TWENTY_EIGHT_MIB => "128MiB",
        _ => "custom",
    }
}

fn chunk_label(size: usize) -> String {
    match size {
        1024 => "chunk_1KiB".to_string(),
        8192 => "chunk_8KiB".to_string(),
        65_536 => "chunk_64KiB".to_string(),
        _ => format!("chunk_{size}B"),
    }
}

fn bench_async_read(c: &mut Criterion) {
    let runtime = runtime();
    for size in payload_sizes() {
        for &chunk_size in chunk_sizes() {
            let name = format!(
                "async_read/{}/{}",
                size_label(size),
                chunk_label(chunk_size)
            );
            c.bench_function(&name, |b| {
                let payload = filled_bytes(size, 0xA5);
                b.to_async(&runtime).iter_batched(
                    move || payload.clone(),
                    move |payload| async move {
                        let body = StreamBody::from_async_read_with_chunk_size(
                            InMemoryAsyncRead::new(payload),
                            chunk_size,
                        )
                        .expect("chunk size");
                        let (bytes, chunks) = drain_transport_stream(
                            DynBody::from_stream_body(body).into_data_stream(),
                        )
                        .await;
                        black_box((bytes, chunks));
                    },
                    BatchSize::LargeInput,
                )
            });
        }
    }
}

fn bench_byte_stream(c: &mut Criterion) {
    let runtime = runtime();
    for size in payload_sizes() {
        for &chunk_size in chunk_sizes() {
            let name = format!(
                "byte_stream/{}/{}",
                size_label(size),
                chunk_label(chunk_size)
            );
            c.bench_function(&name, |b| {
                let payload = filled_bytes(size, 0x5A);
                b.to_async(&runtime).iter_batched(
                    move || {
                        let payload = payload.clone();
                        perf::support::chunked_bytes(payload, chunk_size)
                    },
                    move |chunks| async move {
                        let body = StreamBody::from_byte_stream(ChunkedBodyStream::new(chunks));
                        let (bytes, chunks) = drain_transport_stream(
                            DynBody::from_stream_body(body).into_data_stream(),
                        )
                        .await;
                        black_box((bytes, chunks));
                    },
                    BatchSize::LargeInput,
                )
            });
        }
    }
}

fn streaming_upload(c: &mut Criterion) {
    bench_async_read(c);
    bench_byte_stream(c);
}

criterion_group!(benches, streaming_upload);
criterion_main!(benches);
