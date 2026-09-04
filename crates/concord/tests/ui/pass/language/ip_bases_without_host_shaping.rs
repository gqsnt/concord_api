use concord::Bytes;

concord::api! {
    client Ipv4BaseApi {
        timeout 30s;
        base "http://127.0.0.1:8080/";


    GET read_v4() -> Bytes via Binary;
}}

concord::api! {
    client Ipv6BaseApi {
        timeout 30s;
        base "http://[2001:db8::1]:8080/";


    GET read_v6() -> Bytes via Binary;
}}

fn main() {}
