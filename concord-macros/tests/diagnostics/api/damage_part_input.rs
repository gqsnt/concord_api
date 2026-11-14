concord::api! {
    client Audit {
        base "https://audit.test";
        part supplied(code: String) {}
        GET child(..supplied) { body ; }
    }
}
fn main() {}

include!("../../support/models.rs");
