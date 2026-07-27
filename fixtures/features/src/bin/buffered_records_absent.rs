#[cfg(feature = "fixture-absence")]
use runtime::prelude::BufferedRecords;

fn main() {
    #[cfg(feature = "fixture-absence")]
    let _: Option<BufferedRecords<()>> = None;
}
