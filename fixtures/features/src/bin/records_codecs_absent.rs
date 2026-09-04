#[cfg(feature = "fixture-absence")]
use runtime::records::{
    CommaDelimiter, Csv, CsvDelimiter, CsvHeaderMode, HasHeader, NdJson, NoHeader,
    SemicolonDelimiter, TabDelimiter,
};

fn main() {
    #[cfg(feature = "fixture-absence")]
    {
        let _: Option<Csv> = None;
        let _: Option<NdJson> = None;
        let _: Option<HasHeader> = None;
        let _: Option<NoHeader> = None;
        let _: Option<CommaDelimiter> = None;
        let _: Option<SemicolonDelimiter> = None;
        let _: Option<TabDelimiter> = None;

        fn header_mode<H: CsvHeaderMode>() {}
        fn delimiter<D: CsvDelimiter>() {}
        header_mode::<HasHeader>();
        delimiter::<CommaDelimiter>();
    }
}
