use astra_recheck_probes::drain;
use concord_extras::{
    csv::{CsvOptions, CsvWriter, HeaderMode, Rows, StreamRows},
    stream::from_iter,
};
use serde::{Serialize, Serializer, ser::SerializeStruct};
use std::convert::Infallible;
use std::error::Error;
fn detail(e: concord_core::reqwest::Error) -> String {
    let mut current: &(dyn Error + 'static) = &e;
    loop {
        if let Some(u) = current.downcast_ref::<concord_extras::stream::UploadError>() {
            let c = u.codec().unwrap();
            assert_eq!(c.get("limit"), Some(64));
            return c.to_string();
        }
        current = current.source().unwrap();
    }
}
struct Named(&'static str);
impl Serialize for Named {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut r = s.serialize_struct("Named", 1)?;
        r.serialize_field(self.0, &1)?;
        r.end()
    }
}
fn peak() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find_map(|l| l.strip_prefix("VmHWM:"))
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap()
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let route = args.get(1).map(String::as_str).unwrap_or("inferred");
    let mib: usize = args.get(2).map(|s| s.parse().unwrap()).unwrap_or(16);
    // Preallocate and touch all caller-owned input, including configuration, before baseline.
    let name: &'static str = Box::leak("h".repeat(mib * 1024 * 1024).into_boxed_str());
    let mut opts = CsvOptions::new(HeaderMode::Present, b',')
        .max_header_bytes(64)
        .max_record_bytes(64);
    if route == "configured" || route == "configured-lazy" {
        opts = opts.columns([name]);
    }
    if route == "record" || route == "record-lazy" {
        opts = CsvOptions::new(HeaderMode::Absent, b',').max_record_bytes(64);
    }
    let writer = CsvWriter::new(opts).unwrap();
    let explicit = Rows::new([(1,)]).with_columns([name]);
    std::fs::write("/proc/self/clear_refs", "5").unwrap();
    let before = peak();
    let error = match route {
        "record" => writer.to_vec(Rows::new([(name,)])).unwrap_err().to_string(),
        "record-lazy" => detail(
            drain(
                writer
                    .upload(StreamRows::new(from_iter([Ok::<_, Infallible>((name,))])))
                    .unwrap()
                    .into_parts()
                    .0,
            )
            .unwrap_err(),
        ),
        "inferred" => writer
            .to_vec(Rows::new([Named(name)]))
            .unwrap_err()
            .to_string(),
        "inferred-lazy" => detail(
            drain(
                writer
                    .upload(StreamRows::new(from_iter([Ok::<_, Infallible>(Named(
                        name,
                    ))])))
                    .unwrap()
                    .into_parts()
                    .0,
            )
            .unwrap_err(),
        ),
        "configured" => writer.to_vec(Rows::new([(1,)])).unwrap_err().to_string(),
        "configured-lazy" => detail(
            drain(
                writer
                    .upload(StreamRows::new(from_iter([Ok::<_, Infallible>((1,))])))
                    .unwrap()
                    .into_parts()
                    .0,
            )
            .unwrap_err(),
        ),
        "explicit" => writer.to_vec(explicit).unwrap_err().to_string(),
        _ => panic!("unknown route"),
    };
    println!(
        "route={route} field={mib} MiB limit=64 growth={} KiB error={error}",
        peak().saturating_sub(before)
    );
}
