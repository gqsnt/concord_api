concord::api! {
    pub client BadPort {
        base "http://example.test:not-a-port";
        GET a();
    }
}
concord::api! {
    pub client BadHost {
        base "http://exa mple.test";
        GET a();
    }
}
concord::api! {
    pub client BadV6 {
        base "http://[::g]/";
        GET a();
    }
}
concord::api! {
    pub client BadPath {
        base "http://example.test/a%20b";
        GET a();
    }
}
fn main() {}
concord::api! {
    pub client HexHost {
        base "http://api.0x10";
        GET a();
    }
}
concord::api! {
    pub client Puny {
        base "http://a.b.c.xn--pokxncvks";
        GET a();
    }
}
