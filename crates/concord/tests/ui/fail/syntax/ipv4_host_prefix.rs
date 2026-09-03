concord::api! {
    client Ipv4HostApi {
        timeout off;
        base "http://127.0.0.1:8080/";
        subdomain replace ["api"];


    GET read() -> concord::Bytes using concord::Bytes;
}}

fn main() {}
