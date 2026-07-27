#[cfg(feature = "fixture-absence")]
use runtime::prelude::BufferedRecords as PreludeBufferedRecords;
#[cfg(feature = "fixture-absence")]
use runtime::records::BufferedRecords as ModuleBufferedRecords;

fn main() {
    #[cfg(feature = "fixture-absence")]
    {
        let _: Option<PreludeBufferedRecords<()>> = None;
        let _: Option<ModuleBufferedRecords<()>> = None;
    }
}
