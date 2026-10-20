// Optional values cannot become custom dimensions by correcting a spelling.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client OptionalInlineApi {
        base "https://example.test";
        values { query account?: String; }
        rate_limit {
            id "optional.inline";
            key ["account" = acount];
            allow 1 per 1s;
        }
        GET read() -> Bytes via Binary;
    }
}

api! {
    client OptionalBindingApi {
        base "https://example.test";
        values { query account?: String; }
        rate_limits {
            policy reads(account) {
                id "optional.binding";
                key ["account" = account];
                allow 1 per 1s;
            }
        }
        rate_limit reads(account = acount);
        GET read() -> Bytes via Binary;
    }
}

api! {
    client SensitiveOptionalInlineApi {
        base "https://example.test";
        values { #[sensitive] query account?: String; }
        rate_limit {
            id "sensitive.optional.inline";
            key ["account" = acount];
            allow 1 per 1s;
        }
        GET read() -> Bytes via Binary;
    }
}

api! {
    client SensitiveOptionalBindingApi {
        base "https://example.test";
        values { #[sensitive] query account?: String; }
        rate_limits {
            policy reads(account) {
                id "sensitive.optional.binding";
                key ["account" = account];
                allow 1 per 1s;
            }
        }
        rate_limit reads(account = acount);
        GET read() -> Bytes via Binary;
    }
}

api! {
    client OptionalQualifiedInlineApi {
        base "https://example.test";
        values { query account?: String; }
        GET read() -> Bytes via Binary {
            rate_limit {
                id "optional.qualified.inline";
                key ["account" = client.acount];
                allow 1 per 1s;
            }
        }
    }
}

api! {
    client OptionalAncestorBindingApi {
        base "https://example.test";
        values { query account?: String; }
        rate_limits {
            policy reads(account) {
                id "optional.ancestor.binding";
                key ["account" = account];
                allow 1 per 1s;
            }
        }
        GET read() -> Bytes via Binary {
            rate_limit reads(account = acount);
        }
    }
}

api! {
    client SensitiveRequiredInlineApi {
        base "https://example.test";
        values { #[sensitive] query account: String; }
        rate_limit {
            id "sensitive.required.inline";
            key ["account" = acount];
            allow 1 per 1s;
        }
        GET read() -> Bytes via Binary;
    }
}

fn main() {}
