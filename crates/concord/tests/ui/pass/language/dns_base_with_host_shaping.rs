// §5.4 / §3.3: a DNS base accepts generated subdomain labels. In v3 only a
// client or scope owns `subdomain` (an endpoint-level subdomain is rejected,
// see fail/syntax/forbidden_owner_clauses.rs), so the v2 endpoint clause moves
// to a scope, which qualifies the client value explicitly (LANG-03).
use concord::{Binary, Bytes};

concord::api! {
    client DnsHostApi {
        timeout 30s;
        base "http://example.test/";
        values {
            region: String;
        }

        scope regional() {
            subdomain replace [client.region, "api"];

            GET read() -> Bytes via Binary;
        }
    }
}

fn main() {
    let client = DnsHostApi::builder().set_region("eu").build().expect("client");
    let _: dns_host_api::regional::ReadRequest = client.regional().read();
}
