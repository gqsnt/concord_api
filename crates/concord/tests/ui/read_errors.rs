// `#[read]` states what the method does not: on a read method, below another `#[read]`, or on a
// scope whose endpoints all read by their method, it is an error at the attribute. It takes no
// arguments, appears once, and is not a client attribute.
concord::api! {
    pub client Redundant {
        base "http://x.test";
        #[read] GET get();
        #[read]
        scope account {
            #[read] POST balance();
            #[read]
            scope nested { POST trades(); }
        }
        #[read]
        scope reads { GET one(); HEAD two(); }
    }
}
concord::api! {
    pub client Args {
        base "http://x.test";
        #[read(always)] POST search();
    }
}
concord::api! {
    pub client Root {
        base "http://x.test";
        #[read]
        #[read]
        POST search();
    }
}
concord::api! {
    #[read]
    pub client Whole {
        base "http://x.test";
        POST search();
    }
}
fn main() {}
