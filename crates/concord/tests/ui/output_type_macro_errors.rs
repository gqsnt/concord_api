// F02 (4.2): a type macro is an output type, but the removed in-place spellings around one are
// still errors naming their replacement, and a malformed macro type is rustc's own parse error.
pub struct Item;
pub struct Csv;
macro_rules! item { () => { Item }; }
concord::api! {
    pub client FormatBeforeMacro {
        base "http://x.test";
        GET a() -> Csv item!();
    }
}
concord::api! {
    pub client FormatAfterMacro {
        base "http://x.test";
        GET a() -> item!() Csv;
    }
}
concord::api! {
    pub client MacroVia {
        base "http://x.test";
        GET a() -> item!() via Csv;
    }
}
concord::api! {
    pub client MacroWithoutDelimiter {
        base "http://x.test";
        GET a() -> item! { response Csv; }
    }
}
concord::api! {
    pub client MacroWithoutFormat {
        base "http://x.test";
        GET a() -> item!();
    }
}
fn main() {}
