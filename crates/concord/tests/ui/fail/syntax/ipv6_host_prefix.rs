concord::api! {
    client Ipv6HostApi {
        timeout 30s;
        base "http://[2001:db8::1]:8080/";


    GET read() -> concord::Bytes via concord::Bytes {
        subdomain replace ["api"];
    }
}}

fn main() {}
