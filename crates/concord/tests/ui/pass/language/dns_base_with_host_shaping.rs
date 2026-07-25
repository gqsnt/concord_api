use concord::Bytes;

concord::api! {
    client DnsHostApi {
        base "https://example.test/";
        vars {
            region: String;
        }
    }

    GET read() -> Bytes {
        host [vars.region, "api"];
    }
}

fn main() {}
