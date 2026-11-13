concord::api! {
    client Audit {
        base "https://audit.test";
        status { 600 { bodi } => body }
        GET child() { status { remove 200 } }
    }
}
fn main() {}

include!("../support/real.rs");
