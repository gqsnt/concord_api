// Credential sources suggest complete valid root-relative endpoint paths.
#![allow(unused_imports)]

use concord::{Binary, Bytes, Text, api};

api! {
    client SourceEndpointTypoApi {
        base "https://example.test";
        credentials { credential session = endpoint accounts::logni; }
        scope accounts() {
            POST login() -> Bytes via Binary;
        }
    }
}

api! {
    client SourceScopeTypoApi {
        base "https://example.test";
        credentials { credential session = endpoint acounts::login; }
        scope accounts() {
            POST login() -> Bytes via Binary;
        }
    }
}

api! {
    client SourceEndpointRoleApi {
        base "https://example.test";
        credentials { credential session = endpoint logni; }
        scope login() {
            POST exchange() -> Bytes via Binary;
        }
    }
}

api! {
    client SourceScopeRoleApi {
        base "https://example.test";
        credentials { credential session = endpoint logni::exchange; }
        POST login() -> Bytes via Binary;
    }
}

api! {
    client SourceEndpointUnrelatedApi {
        base "https://example.test";
        credentials { credential session = endpoint unrelated; }
        POST login() -> Bytes via Binary;
    }
}

api! {
    client SourceEndpointIncompleteApi {
        base "https://example.test";
        credentials { credential session = endpoint logni; }
        POST login() -> Option<String> via Text { absent on 404; }
    }
}

api! {
    client RawSourceEndpointTypoApi {
        base "https://example.test";
        credentials { credential session = endpoint tpye; }
        POST r#type() -> Bytes via Binary;
    }
}

api! {
    client RawSourceScopeTypoApi {
        base "https://example.test";
        credentials { credential session = endpoint tpye::login; }
        scope r#type() {
            POST login() -> Bytes via Binary;
        }
    }
}

api! {
    client SourceRemainingPathInvalidApi {
        base "https://example.test";
        credentials { credential session = endpoint acounts::missing; }
        scope accounts() {
            POST login() -> Bytes via Binary;
        }
    }
}

api! {
    client SourceTwoTyposApi {
        base "https://example.test";
        credentials { credential session = endpoint acounts::logni; }
        scope accounts() {
            POST login() -> Bytes via Binary;
        }
    }
}

api! {
    client RawNestedEndpointTypoApi {
        base "https://example.test";
        credentials { credential session = endpoint accounts::tpye; }
        scope accounts() {
            POST r#type() -> Bytes via Binary;
        }
    }
}

api! {
    client RawNestedScopeTypoApi {
        base "https://example.test";
        credentials { credential session = endpoint accounts::tpye::login; }
        scope accounts() {
            scope r#type() {
                POST login() -> Bytes via Binary;
            }
        }
    }
}

fn main() {}
