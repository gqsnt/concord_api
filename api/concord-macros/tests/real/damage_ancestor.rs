concord::api! {
    client Audit {
        read First;
        read Second;
        GET child() {
            query { override "count" = 1; remove "after"; }
            headers { override "x-mode" = "a"; remove "x-tag"; }
            status { override 200 => (), remove 404 }
        }
    }
}
fn main() {}

include!("../support/real.rs");
