#[cfg(feature = "fixture-absence")]
use runtime::records::{Csv, NdJson};

fn main() {
    #[cfg(feature = "fixture-absence")]
    {
        let _: Option<Csv<Vec<String>>> = None;
        let _: Option<NdJson<u32>> = None;
    }
}
