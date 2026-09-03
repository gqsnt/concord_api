use concord::Bytes;

concord::api! {
    client DnsHostApi {
        timeout off;
        base "https://example.test/";
        values {
            region: String;
        }


    GET read() -> Bytes using Bytes {
        subdomain replace [client.region, "api"];
    }
}}

fn main() {}
