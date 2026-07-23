use concord_macros::api;

api! {
    client MissingBlock {
        base "https://example.test";
    }

    GET Read(offset: u64) -> Json<Vec<u8>> {
        paginate crate::Controller
    }
}

api! {
    client OldAssignment {
        base "https://example.test";
    }

    GET Read(offset: u64) -> Json<Vec<u8>> {
        paginate crate::Controller { state = ep.offset }
    }
}

api! {
    client OldCustomFactory {
        base "https://example.test";
    }

    GET Read() -> Json<Vec<u8>> {
        paginate custom crate::factory();
    }
}

api! {
    client DuplicateField {
        base "https://example.test";
    }

    GET Read(offset: u64, other: u64) -> Json<Vec<u8>> {
        paginate crate::Controller {
            state: bind ep.offset,
            state: bind ep.other,
        }
    }
}

api! {
    client DuplicateBindingDestination {
        base "https://example.test";
    }

    GET Read(offset: u64) -> Json<Vec<u8>> {
        paginate crate::Controller {
            state: bind ep.offset,
            other: bind ep.offset,
        }
    }
}

api! {
    client InvalidBindingNamespaces {
        base "https://example.test";
        vars {
            offset: u64;
        }
    }

    GET Read(offset: u64) -> Json<Vec<u8>> {
        paginate crate::Controller { state: bind vars.offset }
    }
}

api! {
    client SecretBinding {
        base "https://example.test";
    }

    GET Read(offset: u64) -> Json<Vec<u8>> {
        paginate crate::Controller { state: bind secret.offset }
    }
}

api! {
    client CredentialBinding {
        base "https://example.test";
    }

    GET Read(offset: u64) -> Json<Vec<u8>> {
        paginate crate::Controller { state: bind credential.offset }
    }
}

api! {
    client IndirectBinding {
        base "https://example.test";
    }

    GET Read(offset: u64) -> Json<Vec<u8>> {
        paginate crate::Controller { state: bind ep.offset.value }
    }
}

api! {
    client ShorthandField {
        base "https://example.test";
    }

    GET Read() -> Json<Vec<u8>> {
        paginate crate::Controller { state }
    }
}

api! {
    client RestConstruction {
        base "https://example.test";
    }

    GET Read() -> Json<Vec<u8>> {
        paginate crate::Controller { .. }
    }
}

api! {
    client CallInitializer {
        base "https://example.test";
    }

    GET Read() -> Json<Vec<u8>> {
        paginate crate::Controller { state: crate::make() }
    }
}

api! {
    client MethodInitializer {
        base "https://example.test";
    }

    GET Read(value: String) -> Json<Vec<u8>> {
        paginate crate::Controller { state: ep.value.clone() }
    }
}

api! {
    client ClosureInitializer {
        base "https://example.test";
    }

    GET Read() -> Json<Vec<u8>> {
        paginate crate::Controller { state: || 1 }
    }
}

api! {
    client AsyncInitializer {
        base "https://example.test";
    }

    GET Read() -> Json<Vec<u8>> {
        paginate crate::Controller { state: async { 1 } }
    }
}

api! {
    client BlockInitializer {
        base "https://example.test";
    }

    GET Read() -> Json<Vec<u8>> {
        paginate crate::Controller { state: { 1 } }
    }
}

api! {
    client FormatInitializer {
        base "https://example.test";
    }

    GET Read() -> Json<Vec<u8>> {
        paginate crate::Controller { state: format_args!("value") }
    }
}

api! {
    client PaginationClauseOrder {
        base "https://example.test";
    }

    GET Read() -> Json<Vec<u8>> {
        paginate crate::Controller {}
        timeout 1s;
    }
}

fn main() {}
