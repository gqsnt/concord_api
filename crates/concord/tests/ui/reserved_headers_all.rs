concord::api! {
    pub client Api {
        base "http://x.test";
        GET a() { headers { "Host" = "x"; } }
        GET b() { headers { "Content-Length" = "0"; } }
        GET c() { headers { "Transfer-Encoding" = "chunked"; } }
        GET d() { headers { "Connection" = "close"; } }
        GET e() { headers { "TE" = "trailers"; } }
        GET f() { headers { "Trailer" = "x"; } }
        GET g() { headers { "Upgrade" = "h2c"; } }
        GET h() { headers { "Keep-Alive" = "timeout=5"; } }
    }
}
fn main() {}
