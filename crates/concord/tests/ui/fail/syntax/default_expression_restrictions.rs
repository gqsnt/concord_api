// §3.1 stable_value: a literal, unary-negative numeric literal or a constant
// path. No call, method, macro, block, index or collection expression.
// V3-L10: bodies have no initializer of any form (`= value` or `#[default]`).
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client CallDefault {
        base "https://example.test";
        values {
            query value: String = String::new();
        }
    }
}

api! {
    client MethodDefault {
        base "https://example.test";
        values {
            query value: String = crate::TEXT.to_owned();
        }
    }
}

api! {
    client MacroDefault {
        base "https://example.test";
        values {
            query value: Vec<u8> = vec![];
        }
    }
}

api! {
    client BlockDefault {
        base "https://example.test";
        values {
            query value: u8 = { 0 };
        }
    }
}

api! {
    client CollectionDefault {
        base "https://example.test";
        values {
            query value: Vec<u8> = [0];
        }
    }
}

api! {
    client RetiredDefaultKeyword {
        base "https://example.test";
        values {
            query value: String = default;
        }
    }
}

api! {
    client DefaultPlusInitializer {
        base "https://example.test";
        values {
            #[default] query value: String = "x";
        }
    }
}

api! {
    client BodyInitializer {
        base "https://example.test";

        POST send(body payload: Bytes via Binary = crate::EMPTY) -> Bytes via Binary;
    }
}

api! {
    client BodyTraitDefault {
        base "https://example.test";

        POST send(#[default] body payload: Bytes via Binary) -> Bytes via Binary;
    }
}

const TEXT: &str = "text";
const EMPTY: Bytes = Bytes::new();

fn main() {}
