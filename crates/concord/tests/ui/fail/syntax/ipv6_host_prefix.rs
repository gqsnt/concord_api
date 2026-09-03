concord::api! {
    client Ipv6HostApi {
        timeout off;
        base "http://[2001:db8::1]:8080/";


    GET read() -> concord::Bytes using concord::Bytes {
        subdomain replace ["api"];
    }
}}

fn main() {}
