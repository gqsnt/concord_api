use concord::api;

#[path = "../../../support/mod.rs"]
mod support;
#[allow(unused_imports)]
use support::Json;

api! {
    client MissingBlock {
        timeout 30s;
        base "https://example.test";


    GET read(offset: u64) -> Vec<u8> via crate::support::Json {
        paginate crate::Controller
    }
}}

api! {
    client OldAssignment {
        timeout 30s;
        base "https://example.test";


    GET read(offset: u64) -> Vec<u8> via crate::support::Json {
        paginate via crate::Controller { state = endpoint.offset }
    }
}}

api! {
    client OldCustomFactory {
        timeout 30s;
        base "https://example.test";


    GET read() -> Vec<u8> via crate::support::Json {
        paginate custom crate::factory();
    }
}}

api! {
    client DuplicateField {
        timeout 30s;
        base "https://example.test";


    GET read(offset: u64, other: u64) -> Vec<u8> via crate::support::Json {
        paginate via crate::Controller {
            state -> endpoint.offset;
            state -> endpoint.other;
        }
    }
}}

api! {
    client DuplicateBindingDestination {
        timeout 30s;
        base "https://example.test";


    GET read(offset: u64) -> Vec<u8> via crate::support::Json {
        paginate via crate::Controller {
            state -> endpoint.offset;
            other -> endpoint.offset;
        }
    }
}}

api! {
    client InvalidBindingNamespaces {
        timeout 30s;
        base "https://example.test";
        values {
            offset: u64;
        }


    GET read(offset: u64) -> Vec<u8> via crate::support::Json {
        paginate via crate::Controller { state -> client.offset; }
    }
}}

api! {
    client SecretBinding {
        timeout 30s;
        base "https://example.test";


    GET read(offset: u64) -> Vec<u8> via crate::support::Json {
        paginate via crate::Controller { state -> secret.offset; }
    }
}}

api! {
    client CredentialBinding {
        timeout 30s;
        base "https://example.test";


    GET read(offset: u64) -> Vec<u8> via crate::support::Json {
        paginate via crate::Controller { state -> credential.offset; }
    }
}}

api! {
    client IndirectBinding {
        timeout 30s;
        base "https://example.test";


    GET read(offset: u64) -> Vec<u8> via crate::support::Json {
        paginate via crate::Controller { state -> endpoint.offset.value; }
    }
}}

api! {
    client ShorthandField {
        timeout 30s;
        base "https://example.test";


    GET read() -> Vec<u8> via crate::support::Json {
        paginate via crate::Controller { state }
    }
}}

api! {
    client RestConstruction {
        timeout 30s;
        base "https://example.test";


    GET read() -> Vec<u8> via crate::support::Json {
        paginate via crate::Controller { .. }
    }
}}

api! {
    client CallInitializer {
        timeout 30s;
        base "https://example.test";


    GET read() -> Vec<u8> via crate::support::Json {
        paginate via crate::Controller { state = crate::make(); }
    }
}}

api! {
    client MethodInitializer {
        timeout 30s;
        base "https://example.test";


    GET read(value: String) -> Vec<u8> via crate::support::Json {
        paginate via crate::Controller { state = endpoint.value.clone(); }
    }
}}

api! {
    client ClosureInitializer {
        timeout 30s;
        base "https://example.test";


    GET read() -> Vec<u8> via crate::support::Json {
        paginate via crate::Controller { state = || 1; }
    }
}}

api! {
    client AsyncInitializer {
        timeout 30s;
        base "https://example.test";


    GET read() -> Vec<u8> via crate::support::Json {
        paginate via crate::Controller { state = async { 1 }; }
    }
}}

api! {
    client BlockInitializer {
        timeout 30s;
        base "https://example.test";


    GET read() -> Vec<u8> via crate::support::Json {
        paginate via crate::Controller { state = { 1 }; }
    }
}}

api! {
    client FormatInitializer {
        timeout 30s;
        base "https://example.test";


    GET read() -> Vec<u8> via crate::support::Json {
        paginate via crate::Controller { state = format_args!("value"); }
    }
}}

api! {
    client TraitDefaultInitializer {
        timeout 30s;
        base "https://example.test";


    GET read() -> Vec<u8> via crate::support::Json {
        paginate via crate::Controller { state = default; }
    }
}}

fn main() {}
