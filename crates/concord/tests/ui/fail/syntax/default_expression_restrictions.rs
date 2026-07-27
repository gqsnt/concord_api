use concord::api;

api! {
    client CallDefault {
        base "https://example.test";
        vars {
            value: String = String::new();
        }
    }
}

api! {
    client MethodDefault {
        base "https://example.test";
        vars {
            value: String = crate::TEXT.to_owned();
        }
    }
}

api! {
    client MacroDefault {
        base "https://example.test";
        vars {
            value: Vec<u8> = vec![];
        }
    }
}

api! {
    client BlockDefault {
        base "https://example.test";
        vars {
            value: u8 = { 0 };
        }
    }
}

api! {
    client CollectionDefault {
        base "https://example.test";
        vars {
            value: Vec<u8> = [0];
        }
    }
}

api! {
    client BodyDefault {
        base "https://example.test";
    }

    POST send(#[body] payload: Bytes = default) -> Bytes;
}

const TEXT: &str = "text";

fn main() {}
