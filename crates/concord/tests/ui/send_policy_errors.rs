// `send` is a client statement, set once, before children, naming a `SendWith` type.
struct NotAPolicy;
concord::api! {
    pub client Scoped {
        base "http://x.test";
        scope s { send NotAPolicy; GET one(); }
    }
}
concord::api! {
    pub client Twice {
        base "http://x.test";
        send NotAPolicy;
        send NotAPolicy;
        GET one();
    }
}
concord::api! {
    pub client Late {
        base "http://x.test";
        GET one();
        send NotAPolicy;
    }
}
concord::api! {
    pub client Empty {
        base "http://x.test";
        send;
        GET one();
    }
}
fn main() {}
