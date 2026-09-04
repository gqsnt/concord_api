#![forbid(unsafe_code)]

use std::hint::black_box;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

const CONSTRUCTIONS: usize = 250_000;
const BYTE_TOTAL: usize = 32 * 1024 * 1024;
const BYTE_REPEATS: usize = 8;
const ITEM_COUNT: usize = 5_000_000;
const SAMPLES: usize = 12;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Step<'a> {
    Pending,
    Item(&'a [u8]),
    Error,
    End,
}

trait Driver {
    fn poll_next(&mut self) -> Step<'_>;
}

struct SliceDriver<'a> {
    chunks: &'a [&'a [u8]],
    index: usize,
    pending_each: bool,
    pending: bool,
    fail_at: Option<usize>,
    terminated: bool,
    drops: Option<&'a AtomicUsize>,
}

impl<'a> SliceDriver<'a> {
    fn new(chunks: &'a [&'a [u8]]) -> Self {
        Self {
            chunks,
            index: 0,
            pending_each: false,
            pending: false,
            fail_at: None,
            terminated: false,
            drops: None,
        }
    }

    fn lifecycle(chunks: &'a [&'a [u8]], drops: &'a AtomicUsize) -> Self {
        Self {
            pending_each: true,
            pending: true,
            fail_at: Some(1),
            drops: Some(drops),
            ..Self::new(chunks)
        }
    }
}

impl Driver for SliceDriver<'_> {
    fn poll_next(&mut self) -> Step<'_> {
        if self.terminated {
            return Step::End;
        }
        if self.pending_each && self.pending {
            self.pending = false;
            return Step::Pending;
        }
        if self.fail_at == Some(self.index) {
            self.terminated = true;
            return Step::Error;
        }
        let Some(item) = self.chunks.get(self.index).copied() else {
            self.terminated = true;
            return Step::End;
        };
        self.index += 1;
        self.pending = self.pending_each;
        Step::Item(item)
    }
}

impl Drop for SliceDriver<'_> {
    fn drop(&mut self) {
        if let Some(drops) = self.drops {
            drops.fetch_add(1, Ordering::SeqCst);
        }
    }
}

enum EnumDriver<'a> {
    Slice(SliceDriver<'a>),
}

struct EmptyDriver;

impl Driver for EmptyDriver {
    fn poll_next(&mut self) -> Step<'_> {
        Step::End
    }
}

#[inline(never)]
fn boxed_driver<'a>(chunks: &'a [&'a [u8]]) -> Box<dyn Driver + 'a> {
    if black_box(chunks.len()) == usize::MAX {
        Box::new(EmptyDriver)
    } else {
        Box::new(SliceDriver::new(chunks))
    }
}

impl Driver for EnumDriver<'_> {
    fn poll_next(&mut self) -> Step<'_> {
        match self {
            Self::Slice(driver) => driver.poll_next(),
        }
    }
}

#[derive(Clone, Copy)]
enum Workload {
    Bytes,
    NdJson,
    Csv,
}

#[derive(serde::Deserialize)]
struct NdJsonRecord {
    id: u64,
}

fn consume<D: Driver + ?Sized>(driver: &mut D, workload: Workload) -> u64 {
    let mut checksum = 0_u64;
    let mut csv_reader = csv_core::Reader::new();
    let mut csv_output = [0_u8; 64];
    let mut csv_ends = [0_usize; 4];
    loop {
        match driver.poll_next() {
            Step::Pending => std::hint::spin_loop(),
            Step::Item(item) => {
                let item = black_box(item);
                checksum = match workload {
                    Workload::Bytes => item.iter().fold(checksum, |sum, byte| {
                        sum.wrapping_mul(31).wrapping_add(u64::from(*byte))
                    }),
                    Workload::NdJson => {
                        let record: NdJsonRecord =
                            serde_json::from_slice(item).expect("valid benchmark NDJSON record");
                        checksum.wrapping_add(record.id)
                    }
                    Workload::Csv => {
                        let (result, _, _, fields) =
                            csv_reader.read_record(item, &mut csv_output, &mut csv_ends);
                        assert_eq!(result, csv_core::ReadRecordResult::Record);
                        assert_eq!(fields, 2);
                        let first = std::str::from_utf8(&csv_output[..csv_ends[0]])
                            .expect("ASCII benchmark CSV field")
                            .parse::<u64>()
                            .expect("numeric benchmark CSV field");
                        checksum.wrapping_add(first)
                    }
                };
            }
            Step::Error | Step::End => return checksum,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ItemStep {
    Pending,
    Item(u64),
    Error,
    End,
}

trait ItemDriver {
    fn poll_item(&mut self) -> ItemStep;
}

struct ParsedDriver<'a> {
    source: SliceDriver<'a>,
    workload: Workload,
    csv_reader: csv_core::Reader,
    csv_output: [u8; 64],
    csv_ends: [usize; 4],
}

impl<'a> ParsedDriver<'a> {
    fn new(chunks: &'a [&'a [u8]], workload: Workload) -> Self {
        Self {
            source: SliceDriver::new(chunks),
            workload,
            csv_reader: csv_core::Reader::new(),
            csv_output: [0; 64],
            csv_ends: [0; 4],
        }
    }

    fn next_item(&mut self) -> ItemStep {
        let item = match self.source.poll_next() {
            Step::Pending => return ItemStep::Pending,
            Step::Error => return ItemStep::Error,
            Step::End => return ItemStep::End,
            Step::Item(item) => item,
        };
        let item = black_box(item);
        let value = match self.workload {
            Workload::NdJson => {
                let record: NdJsonRecord =
                    serde_json::from_slice(item).expect("valid benchmark NDJSON record");
                record.id
            }
            Workload::Csv => {
                let (result, _, _, fields) =
                    self.csv_reader
                        .read_record(item, &mut self.csv_output, &mut self.csv_ends);
                assert_eq!(result, csv_core::ReadRecordResult::Record);
                assert_eq!(fields, 2);
                std::str::from_utf8(&self.csv_output[..self.csv_ends[0]])
                    .expect("ASCII benchmark CSV field")
                    .parse::<u64>()
                    .expect("numeric benchmark CSV field")
            }
            Workload::Bytes => unreachable!("semantic driver requires a record workload"),
        };
        ItemStep::Item(value)
    }
}

impl ItemDriver for ParsedDriver<'_> {
    fn poll_item(&mut self) -> ItemStep {
        self.next_item()
    }
}

enum ParsedEnum<'a> {
    Parsed(ParsedDriver<'a>),
}

impl ItemDriver for ParsedEnum<'_> {
    fn poll_item(&mut self) -> ItemStep {
        match self {
            Self::Parsed(driver) => driver.poll_item(),
        }
    }
}

struct EmptyItemDriver;

impl ItemDriver for EmptyItemDriver {
    fn poll_item(&mut self) -> ItemStep {
        ItemStep::End
    }
}

#[inline(never)]
fn boxed_item_driver<'a>(chunks: &'a [&'a [u8]], workload: Workload) -> Box<dyn ItemDriver + 'a> {
    if black_box(chunks.len()) == usize::MAX {
        Box::new(EmptyItemDriver)
    } else {
        Box::new(ParsedDriver::new(chunks, workload))
    }
}

fn consume_items<D: ItemDriver + ?Sized>(driver: &mut D) -> u64 {
    let mut checksum = 0_u64;
    loop {
        match driver.poll_item() {
            ItemStep::Pending => std::hint::spin_loop(),
            ItemStep::Item(value) => checksum = checksum.wrapping_mul(31).wrapping_add(value),
            ItemStep::Error | ItemStep::End => return checksum,
        }
    }
}

#[derive(Clone, Copy)]
struct Timings {
    static_driver: Duration,
    enum_driver: Duration,
    boxed_driver: Duration,
}

impl Timings {
    fn fastest(self) -> Duration {
        self.static_driver
            .min(self.enum_driver)
            .min(self.boxed_driver)
    }
}

fn median_timings(mut sample: impl FnMut() -> Timings) -> Timings {
    let mut static_values = Vec::with_capacity(SAMPLES);
    let mut enum_values = Vec::with_capacity(SAMPLES);
    let mut boxed_values = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let timings = sample();
        static_values.push(timings.static_driver);
        enum_values.push(timings.enum_driver);
        boxed_values.push(timings.boxed_driver);
    }
    static_values.sort_unstable();
    enum_values.sort_unstable();
    boxed_values.sort_unstable();
    Timings {
        static_driver: static_values[SAMPLES / 2],
        enum_driver: enum_values[SAMPLES / 2],
        boxed_driver: boxed_values[SAMPLES / 2],
    }
}

fn median_throughput_samples(mut sample: impl FnMut() -> Timings) -> (Timings, f64) {
    let mut static_values = Vec::with_capacity(SAMPLES);
    let mut enum_values = Vec::with_capacity(SAMPLES);
    let mut boxed_values = Vec::with_capacity(SAMPLES);
    let mut boxed_ratios = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        let timings = sample();
        static_values.push(timings.static_driver);
        enum_values.push(timings.enum_driver);
        boxed_values.push(timings.boxed_driver);
        boxed_ratios.push(timings.fastest().as_secs_f64() / timings.boxed_driver.as_secs_f64());
    }
    static_values.sort_unstable();
    enum_values.sort_unstable();
    boxed_values.sort_unstable();
    boxed_ratios.sort_by(f64::total_cmp);
    let medians = Timings {
        static_driver: static_values[SAMPLES / 2],
        enum_driver: enum_values[SAMPLES / 2],
        boxed_driver: boxed_values[SAMPLES / 2],
    };
    (medians, boxed_ratios[SAMPLES / 2])
}

fn construction_timings(chunks: &[&[u8]]) -> Timings {
    let started = Instant::now();
    for _ in 0..CONSTRUCTIONS {
        black_box(SliceDriver::new(chunks));
    }
    let static_driver = started.elapsed();

    let started = Instant::now();
    for _ in 0..CONSTRUCTIONS {
        black_box(EnumDriver::Slice(SliceDriver::new(chunks)));
    }
    let enum_driver = started.elapsed();

    let started = Instant::now();
    for _ in 0..CONSTRUCTIONS {
        let driver = boxed_driver(chunks);
        black_box(driver);
    }
    let boxed_driver = started.elapsed();
    Timings {
        static_driver,
        enum_driver,
        boxed_driver,
    }
}

fn first_item_timings(chunks: &[&[u8]]) -> Timings {
    let started = Instant::now();
    for _ in 0..CONSTRUCTIONS {
        let mut driver = SliceDriver::new(chunks);
        black_box(driver.poll_next());
    }
    let static_driver = started.elapsed();

    let started = Instant::now();
    for _ in 0..CONSTRUCTIONS {
        let mut driver = EnumDriver::Slice(SliceDriver::new(chunks));
        black_box(driver.poll_next());
    }
    let enum_driver = started.elapsed();

    let started = Instant::now();
    for _ in 0..CONSTRUCTIONS {
        let mut driver = boxed_driver(chunks);
        black_box(driver.poll_next());
    }
    let boxed_driver = started.elapsed();
    Timings {
        static_driver,
        enum_driver,
        boxed_driver,
    }
}

fn static_throughput(chunks: &[&[u8]], repeats: usize, workload: Workload) -> (Duration, u64) {
    let started = Instant::now();
    let mut sum = 0_u64;
    for _ in 0..repeats {
        sum ^= consume(&mut SliceDriver::new(chunks), workload);
    }
    black_box(sum);
    (started.elapsed(), sum)
}

fn enum_throughput(chunks: &[&[u8]], repeats: usize, workload: Workload) -> (Duration, u64) {
    let started = Instant::now();
    let mut sum = 0_u64;
    for _ in 0..repeats {
        sum ^= consume(&mut EnumDriver::Slice(SliceDriver::new(chunks)), workload);
    }
    black_box(sum);
    (started.elapsed(), sum)
}

fn boxed_throughput(chunks: &[&[u8]], repeats: usize, workload: Workload) -> (Duration, u64) {
    let started = Instant::now();
    let mut sum = 0_u64;
    for _ in 0..repeats {
        let mut driver = boxed_driver(chunks);
        sum ^= consume(driver.as_mut(), workload);
    }
    black_box(sum);
    (started.elapsed(), sum)
}

fn throughput_timings(
    chunks: &[&[u8]],
    repeats: usize,
    workload: Workload,
    order: usize,
) -> Timings {
    let (static_result, enum_result, boxed_result) = match order % 3 {
        0 => (
            static_throughput(chunks, repeats, workload),
            enum_throughput(chunks, repeats, workload),
            boxed_throughput(chunks, repeats, workload),
        ),
        1 => {
            let enum_result = enum_throughput(chunks, repeats, workload);
            let boxed_result = boxed_throughput(chunks, repeats, workload);
            let static_result = static_throughput(chunks, repeats, workload);
            (static_result, enum_result, boxed_result)
        }
        _ => {
            let boxed_result = boxed_throughput(chunks, repeats, workload);
            let static_result = static_throughput(chunks, repeats, workload);
            let enum_result = enum_throughput(chunks, repeats, workload);
            (static_result, enum_result, boxed_result)
        }
    };
    let (static_driver, static_sum) = static_result;
    let (enum_driver, enum_sum) = enum_result;
    let (boxed_driver, boxed_sum) = boxed_result;
    assert_eq!(static_sum, enum_sum);
    assert_eq!(static_sum, boxed_sum);
    Timings {
        static_driver,
        enum_driver,
        boxed_driver,
    }
}

fn static_item_throughput(chunks: &[&[u8]], workload: Workload) -> (Duration, u64) {
    let started = Instant::now();
    let sum = consume_items(&mut ParsedDriver::new(chunks, workload));
    black_box(sum);
    (started.elapsed(), sum)
}

fn enum_item_throughput(chunks: &[&[u8]], workload: Workload) -> (Duration, u64) {
    let started = Instant::now();
    let sum = consume_items(&mut ParsedEnum::Parsed(ParsedDriver::new(chunks, workload)));
    black_box(sum);
    (started.elapsed(), sum)
}

fn boxed_item_throughput(chunks: &[&[u8]], workload: Workload) -> (Duration, u64) {
    let started = Instant::now();
    let mut driver = boxed_item_driver(chunks, workload);
    let sum = consume_items(driver.as_mut());
    black_box(sum);
    (started.elapsed(), sum)
}

fn item_throughput_timings(chunks: &[&[u8]], workload: Workload, order: usize) -> Timings {
    let (static_result, enum_result, boxed_result) = match order % 3 {
        0 => (
            static_item_throughput(chunks, workload),
            enum_item_throughput(chunks, workload),
            boxed_item_throughput(chunks, workload),
        ),
        1 => {
            let enum_result = enum_item_throughput(chunks, workload);
            let boxed_result = boxed_item_throughput(chunks, workload);
            let static_result = static_item_throughput(chunks, workload);
            (static_result, enum_result, boxed_result)
        }
        _ => {
            let boxed_result = boxed_item_throughput(chunks, workload);
            let static_result = static_item_throughput(chunks, workload);
            let enum_result = enum_item_throughput(chunks, workload);
            (static_result, enum_result, boxed_result)
        }
    };
    let (static_driver, static_sum) = static_result;
    let (enum_driver, enum_sum) = enum_result;
    let (boxed_driver, boxed_sum) = boxed_result;
    assert_eq!(static_sum, enum_sum);
    assert_eq!(static_sum, boxed_sum);
    Timings {
        static_driver,
        enum_driver,
        boxed_driver,
    }
}

fn allocation_counts(chunks: &[&[u8]]) -> [(u64, u64); 3] {
    let static_construction = allocation_counter::measure(|| {
        black_box(SliceDriver::new(chunks));
    });
    let enum_construction = allocation_counter::measure(|| {
        black_box(EnumDriver::Slice(SliceDriver::new(chunks)));
    });
    let boxed_construction = allocation_counter::measure(|| {
        let driver = boxed_driver(chunks);
        black_box(driver);
    });

    let mut static_driver = SliceDriver::new(chunks);
    let static_poll = allocation_counter::measure(|| {
        black_box(static_driver.poll_next());
    });
    let mut enum_driver = EnumDriver::Slice(SliceDriver::new(chunks));
    let enum_poll = allocation_counter::measure(|| {
        black_box(enum_driver.poll_next());
    });
    let mut boxed_driver = boxed_driver(chunks);
    let boxed_poll = allocation_counter::measure(|| {
        black_box(boxed_driver.poll_next());
    });

    [
        (static_construction.count_total, static_poll.count_total),
        (enum_construction.count_total, enum_poll.count_total),
        (boxed_construction.count_total, boxed_poll.count_total),
    ]
}

fn lifecycle_signature(mut driver: impl Driver) -> Vec<&'static str> {
    let mut signature = Vec::new();
    for _ in 0..5 {
        signature.push(match driver.poll_next() {
            Step::Pending => "pending",
            Step::Item(_) => "item",
            Step::Error => "error",
            Step::End => "end",
        });
    }
    signature
}

fn assert_lifecycle_equivalence(chunks: &[&[u8]]) {
    let static_drops = AtomicUsize::new(0);
    let enum_drops = AtomicUsize::new(0);
    let boxed_drops = AtomicUsize::new(0);
    let static_signature = lifecycle_signature(SliceDriver::lifecycle(chunks, &static_drops));
    let enum_signature = lifecycle_signature(EnumDriver::Slice(SliceDriver::lifecycle(
        chunks,
        &enum_drops,
    )));
    let boxed_signature =
        lifecycle_signature(Box::new(SliceDriver::lifecycle(chunks, &boxed_drops)));
    assert_eq!(
        static_signature,
        ["pending", "item", "pending", "error", "end"]
    );
    assert_eq!(static_signature, enum_signature);
    assert_eq!(static_signature, boxed_signature);
    assert_eq!(static_drops.load(Ordering::SeqCst), 1);
    assert_eq!(enum_drops.load(Ordering::SeqCst), 1);
    assert_eq!(boxed_drops.load(Ordering::SeqCst), 1);
}

impl<T: Driver + ?Sized> Driver for Box<T> {
    fn poll_next(&mut self) -> Step<'_> {
        self.as_mut().poll_next()
    }
}

fn ns_per(duration: Duration, count: usize) -> f64 {
    duration.as_secs_f64() * 1_000_000_000.0 / count as f64
}

fn mib_per_second(bytes: usize, duration: Duration) -> f64 {
    bytes as f64 / (1024.0 * 1024.0) / duration.as_secs_f64()
}

fn print_timings(label: &str, timings: Timings, unit: &str, scale: impl Fn(Duration) -> f64) {
    println!(
        "| {label} | {:.2} | {:.2} | {:.2} | {unit} |",
        scale(timings.static_driver),
        scale(timings.enum_driver),
        scale(timings.boxed_driver),
    );
}

fn main() {
    let rustc = Command::new("rustc")
        .arg("--version")
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_else(|error| format!("unavailable: {error}"));
    println!(
        "environment: {rustc}; {}-{}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    println!("samples: {SAMPLES} (reported independently sorted medians)");
    println!(
        "budgets: construction boxed allocations <=1; poll allocations 0; bytes <=10% throughput deficit; items <=15% throughput deficit"
    );

    let first = [b"123456,abcdef\n".as_slice(), b"789012,ghijkl\n".as_slice()];
    let allocations = allocation_counts(&first);
    assert_eq!(allocations[0], (0, 0), "static allocation budget");
    assert_eq!(allocations[1], (0, 0), "enum allocation budget");
    assert!(
        allocations[2].0 <= 1,
        "boxed construction allocation budget"
    );
    assert_eq!(allocations[2].1, 0, "boxed per-item allocation budget");
    assert_lifecycle_equivalence(&first);

    println!("| workload | static | private enum | boxed | unit |");
    println!("|---|---:|---:|---:|---|");
    let construction = median_timings(|| construction_timings(&first));
    print_timings("construction", construction, "ns/op", |value| {
        ns_per(value, CONSTRUCTIONS)
    });
    let first_item = median_timings(|| first_item_timings(&first));
    print_timings("construction + first item", first_item, "ns/op", |value| {
        ns_per(value, CONSTRUCTIONS)
    });

    let mut byte_results = Vec::new();
    for chunk_size in [256, 4_096, 65_536] {
        let payload = vec![0x5a_u8; chunk_size];
        let chunks = vec![payload.as_slice(); BYTE_TOTAL / chunk_size];
        let mut sample = 0;
        let (timings, boxed_ratio) = median_throughput_samples(|| {
            let timings = throughput_timings(&chunks, BYTE_REPEATS, Workload::Bytes, sample);
            sample += 1;
            timings
        });
        let bytes = BYTE_TOTAL * BYTE_REPEATS;
        print_timings(
            &format!("bytes/{chunk_size}"),
            timings,
            "MiB/s",
            |duration| mib_per_second(bytes, duration),
        );
        byte_results.push((timings, boxed_ratio));
    }

    let ndjson = b"{\"id\":123456}\n".as_slice();
    let ndjson_items = vec![ndjson; ITEM_COUNT];
    let mut sample = 0;
    let (ndjson_timings, ndjson_boxed_ratio) = median_throughput_samples(|| {
        let timings = item_throughput_timings(&ndjson_items, Workload::NdJson, sample);
        sample += 1;
        timings
    });
    print_timings("NDJSON items", ndjson_timings, "M items/s", |duration| {
        ITEM_COUNT as f64 / duration.as_secs_f64() / 1_000_000.0
    });
    let csv = b"123456,abcdef\n".as_slice();
    let csv_items = vec![csv; ITEM_COUNT];
    let mut sample = 0;
    let (csv_timings, csv_boxed_ratio) = median_throughput_samples(|| {
        let timings = item_throughput_timings(&csv_items, Workload::Csv, sample);
        sample += 1;
        timings
    });
    print_timings("CSV items", csv_timings, "M items/s", |duration| {
        ITEM_COUNT as f64 / duration.as_secs_f64() / 1_000_000.0
    });

    println!(
        "paired-sample median boxed/fastest throughput ratios: bytes={:.3},{:.3},{:.3}; items={ndjson_boxed_ratio:.3},{csv_boxed_ratio:.3}",
        byte_results[0].1, byte_results[1].1, byte_results[2].1,
    );

    for (_, boxed_ratio) in &byte_results {
        assert!(
            *boxed_ratio >= 0.90,
            "boxed byte driver exceeded the 10% throughput-deficit budget"
        );
    }
    for boxed_ratio in [ndjson_boxed_ratio, csv_boxed_ratio] {
        assert!(
            boxed_ratio >= 0.85,
            "boxed item driver exceeded the 15% throughput-deficit budget"
        );
    }

    println!(
        "allocation counts (construction,poll): static={:?}, enum={:?}, boxed={:?}",
        allocations[0], allocations[1], allocations[2]
    );
    println!(
        "lifecycle signature: pending,item,pending,error,end; early drop observed once for every strategy"
    );
    println!("decision: boxed erasure meets the precommitted runtime/allocation budgets");

    let output = format!(
        "environment={rustc};{}-{}\n\
         samples={SAMPLES};timings=independently-sorted-medians;ratios=paired-sample-medians\n\
         construction_ns={:.2},{:.2},{:.2}\n\
         first_item_ns={:.2},{:.2},{:.2}\n\
         bytes_256_mib_s={:.2},{:.2},{:.2}\n\
         bytes_4096_mib_s={:.2},{:.2},{:.2}\n\
         bytes_65536_mib_s={:.2},{:.2},{:.2}\n\
         ndjson_m_items_s={:.2},{:.2},{:.2}\n\
         csv_m_items_s={:.2},{:.2},{:.2}\n\
         boxed_fastest_ratios={:.3},{:.3},{:.3},{:.3},{:.3}\n\
         allocation_construction_poll={:?},{:?},{:?}\n\
         lifecycle=pending,item,pending,error,end;drop=1,1,1\n\
         decision=boxed-erasure-meets-precommitted-runtime-allocation-budgets\n",
        std::env::consts::OS,
        std::env::consts::ARCH,
        ns_per(construction.static_driver, CONSTRUCTIONS),
        ns_per(construction.enum_driver, CONSTRUCTIONS),
        ns_per(construction.boxed_driver, CONSTRUCTIONS),
        ns_per(first_item.static_driver, CONSTRUCTIONS),
        ns_per(first_item.enum_driver, CONSTRUCTIONS),
        ns_per(first_item.boxed_driver, CONSTRUCTIONS),
        mib_per_second(BYTE_TOTAL * BYTE_REPEATS, byte_results[0].0.static_driver),
        mib_per_second(BYTE_TOTAL * BYTE_REPEATS, byte_results[0].0.enum_driver),
        mib_per_second(BYTE_TOTAL * BYTE_REPEATS, byte_results[0].0.boxed_driver),
        mib_per_second(BYTE_TOTAL * BYTE_REPEATS, byte_results[1].0.static_driver),
        mib_per_second(BYTE_TOTAL * BYTE_REPEATS, byte_results[1].0.enum_driver),
        mib_per_second(BYTE_TOTAL * BYTE_REPEATS, byte_results[1].0.boxed_driver),
        mib_per_second(BYTE_TOTAL * BYTE_REPEATS, byte_results[2].0.static_driver),
        mib_per_second(BYTE_TOTAL * BYTE_REPEATS, byte_results[2].0.enum_driver),
        mib_per_second(BYTE_TOTAL * BYTE_REPEATS, byte_results[2].0.boxed_driver),
        ITEM_COUNT as f64 / ndjson_timings.static_driver.as_secs_f64() / 1_000_000.0,
        ITEM_COUNT as f64 / ndjson_timings.enum_driver.as_secs_f64() / 1_000_000.0,
        ITEM_COUNT as f64 / ndjson_timings.boxed_driver.as_secs_f64() / 1_000_000.0,
        ITEM_COUNT as f64 / csv_timings.static_driver.as_secs_f64() / 1_000_000.0,
        ITEM_COUNT as f64 / csv_timings.enum_driver.as_secs_f64() / 1_000_000.0,
        ITEM_COUNT as f64 / csv_timings.boxed_driver.as_secs_f64() / 1_000_000.0,
        byte_results[0].1,
        byte_results[1].1,
        byte_results[2].1,
        ndjson_boxed_ratio,
        csv_boxed_ratio,
        allocations[0],
        allocations[1],
        allocations[2],
    );
    let output_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/perf/stream-driver-comparison");
    std::fs::create_dir_all(&output_dir).expect("create raw-result directory");
    let output_path = output_dir.join("latest.txt");
    std::fs::write(&output_path, output).expect("write raw-result summary");
    println!("raw-result: {}", output_path.display());
}
