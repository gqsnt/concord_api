// H9: a value placed directly as a header needs `ToHeaderValue`; `Display` alone is not used.
use std::fmt;

pub struct Region;
impl fmt::Display for Region {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("eu")
    }
}

concord::api! {
    pub client Api {
        base "http://x.test";
        GET foreign(header("x-ip") ip: std::net::IpAddr);
        GET local(header("x-region") region: Region);
        GET option(header("x-opt") opt: Option<String>);
        GET float(header("x-ratio") ratio: f64);
    }
}
fn main() {}
