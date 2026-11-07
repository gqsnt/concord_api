concord::api! {
    pub client C() {
        base "https://x.test";
        DELETE remove() { path "x"; limit 1024; }
    }
}

fn main() {}
