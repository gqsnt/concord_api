// A literal header set again below a native key: the error names the owner that set it
// (`Blame`), not the native contribution in between, whose header the macro cannot see.
use concord::reqwest::header::ACCEPT;

concord::api! {
    pub client Blame {
        base "http://x.test";
        headers { "x-a" = "1"; }
        scope s {
            headers { ACCEPT = "application/json"; }
            GET e() { headers { "x-a" = "2"; } }
        }
    }
}

fn main() {}
