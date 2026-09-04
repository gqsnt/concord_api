use concord::Bytes;

concord::api! {
    client DnsHostApi {
        timeout 30s;
        base "https://example.test/";
        values {
            region: String;
        }


    GET read() -> Bytes via Binary {
        subdomain replace [client.region, "api"];
    }
}}

fn main() {}
