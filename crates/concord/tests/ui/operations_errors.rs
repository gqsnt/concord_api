// `operations Name;` is a client statement, set once, before children, naming a new type; every
// endpoint's variant must be a name the enum can have.
concord::api! {
    pub client Scoped {
        base "http://x.test";
        scope s { operations ScopedOperation; GET one(); }
    }
}
concord::api! {
    pub client Twice {
        base "http://x.test";
        operations TwiceOperation;
        operations TwiceOperation;
        GET one();
    }
}
concord::api! {
    pub client Late {
        base "http://x.test";
        GET one();
        operations LateOperation;
    }
}
concord::api! {
    pub client Empty {
        base "http://x.test";
        operations;
        GET one();
    }
}
concord::api! {
    pub client Shop {
        base "http://x.test";
        operations ShopItems;
        scope items { GET list(); }
    }
}
concord::api! {
    pub client Keyed(query key: String) {
        base "http://x.test";
        operations KeyedConfig;
        GET one();
    }
}
concord::api! {
    pub client Grouped {
        base "http://x.test";
        operations Paging;
        params Paging(query page?: u32);
        GET one(..Paging);
    }
}
concord::api! {
    pub client Selfish {
        base "http://x.test";
        operations SelfishOperation;
        GET self_();
    }
}
fn main() {}
