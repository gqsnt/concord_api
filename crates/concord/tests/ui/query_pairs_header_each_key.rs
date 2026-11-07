concord::api! {
    pub client C() {
        base "https://x.test";
        GET a(header(..) h: String) { path "a"; }
    }
}

concord::api! {
    pub client D() {
        base "https://x.test";
        GET b(query(..) each v: Vec<String>) { path "b"; }
    }
}

concord::api! {
    pub client E() {
        base "https://x.test";
        GET c(query(.. "k") v: String) { path "c"; }
    }
}

fn main() {}
