concord::api! {
    pub client Api {
        base "http://x.test";
        GET e() { headers { "x-name" = "café"; } }
        GET f() { headers { "x-ctl" = "a\nb"; } }
    }
}
fn main() {}
