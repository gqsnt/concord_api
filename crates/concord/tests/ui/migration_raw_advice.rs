// F04 (4.2): a removed trailing body format is reported with the body's name as written, so a raw
// identifier stays raw (`body r#type?: String`, which parses; `body type?` would not).
// tests/applied_advice.rs compiles the applied text.
pub struct Utf8;
concord::api! {
    pub client MigrationRaw {
        base "http://x.test";
        POST broken(r#type?: String Utf8);
    }
}
concord::api! {
    pub client MigrationRawVia {
        base "http://x.test";
        POST broken(r#match: String via Utf8);
    }
}
fn main() {}
