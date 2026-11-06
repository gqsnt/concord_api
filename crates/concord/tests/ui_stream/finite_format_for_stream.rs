// `-> stream T` needs an incremental format: a finite one is refused at the format statement.
// (Its own directory: rustc lists the `Incremental` implementors, which depend on features.)
concord::api! {
    pub client Finite {
        base "http://x.test";
        GET events() -> stream String { response concord::Json; }
    }
}
fn main() {}
