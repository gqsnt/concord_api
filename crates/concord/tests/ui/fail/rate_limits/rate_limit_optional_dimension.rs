use concord::api;

api! {
    client Api {
        timeout 30s;
        base "https://example.test";
        values {
            tenant?: String;
        }
        rate_limit { id "concord.rate_limit_optional_dimension.1"; cost 1;  key [client.tenant]; allow 1 per 1s; }


    GET read() -> Bytes via Binary;
}}

fn main() {}
