use concord::api;

#[path = "../../../support/mod.rs"]
mod support;
#[allow(unused_imports)]
use support::Json;

api! {
    client MissingBlock {
        timeout off;
        base "https://example.test";


    GET read(offset: u64) -> Vec<u8> using crate::support::Json<Vec<u8>> {
        paginate crate::Controller
    }
}}

api! {
    client OldAssignment {
        timeout off;
        base "https://example.test";


    GET read(offset: u64) -> Vec<u8> using crate::support::Json<Vec<u8>> {
        paginate with crate::Controller { state = endpoint.offset }
    }
}}

api! {
    client OldCustomFactory {
        timeout off;
        base "https://example.test";


    GET read() -> Vec<u8> using crate::support::Json<Vec<u8>> {
        paginate custom crate::factory();
    }
}}

api! {
    client DuplicateField {
        timeout off;
        base "https://example.test";


    GET read(offset: u64, other: u64) -> Vec<u8> using crate::support::Json<Vec<u8>> {
        paginate with crate::Controller {
            state -> endpoint.offset;
            state -> endpoint.other;
        }
    }
}}

api! {
    client DuplicateBindingDestination {
        timeout off;
        base "https://example.test";


    GET read(offset: u64) -> Vec<u8> using crate::support::Json<Vec<u8>> {
        paginate with crate::Controller {
            state -> endpoint.offset;
            other -> endpoint.offset;
        }
    }
}}

api! {
    client InvalidBindingNamespaces {
        timeout off;
        base "https://example.test";
        values {
            offset: u64;
        }


    GET read(offset: u64) -> Vec<u8> using crate::support::Json<Vec<u8>> {
        paginate with crate::Controller { state -> client.offset; }
    }
}}

api! {
    client SecretBinding {
        timeout off;
        base "https://example.test";


    GET read(offset: u64) -> Vec<u8> using crate::support::Json<Vec<u8>> {
        paginate with crate::Controller { state -> secret.offset; }
    }
}}

api! {
    client CredentialBinding {
        timeout off;
        base "https://example.test";


    GET read(offset: u64) -> Vec<u8> using crate::support::Json<Vec<u8>> {
        paginate with crate::Controller { state -> credential.offset; }
    }
}}

api! {
    client IndirectBinding {
        timeout off;
        base "https://example.test";


    GET read(offset: u64) -> Vec<u8> using crate::support::Json<Vec<u8>> {
        paginate with crate::Controller { state -> endpoint.offset.value; }
    }
}}

api! {
    client ShorthandField {
        timeout off;
        base "https://example.test";


    GET read() -> Vec<u8> using crate::support::Json<Vec<u8>> {
        paginate with crate::Controller { state }
    }
}}

api! {
    client RestConstruction {
        timeout off;
        base "https://example.test";


    GET read() -> Vec<u8> using crate::support::Json<Vec<u8>> {
        paginate with crate::Controller { .. }
    }
}}

api! {
    client CallInitializer {
        timeout off;
        base "https://example.test";


    GET read() -> Vec<u8> using crate::support::Json<Vec<u8>> {
        paginate with crate::Controller { state = crate::make(); }
    }
}}

api! {
    client MethodInitializer {
        timeout off;
        base "https://example.test";


    GET read(value: String) -> Vec<u8> using crate::support::Json<Vec<u8>> {
        paginate with crate::Controller { state = endpoint.value.clone(); }
    }
}}

api! {
    client ClosureInitializer {
        timeout off;
        base "https://example.test";


    GET read() -> Vec<u8> using crate::support::Json<Vec<u8>> {
        paginate with crate::Controller { state = || 1; }
    }
}}

api! {
    client AsyncInitializer {
        timeout off;
        base "https://example.test";


    GET read() -> Vec<u8> using crate::support::Json<Vec<u8>> {
        paginate with crate::Controller { state = async { 1 }; }
    }
}}

api! {
    client BlockInitializer {
        timeout off;
        base "https://example.test";


    GET read() -> Vec<u8> using crate::support::Json<Vec<u8>> {
        paginate with crate::Controller { state = { 1 }; }
    }
}}

api! {
    client FormatInitializer {
        timeout off;
        base "https://example.test";


    GET read() -> Vec<u8> using crate::support::Json<Vec<u8>> {
        paginate with crate::Controller { state = format_args!("value"); }
    }
}}

api! {
    client TraitDefaultInitializer {
        timeout off;
        base "https://example.test";


    GET read() -> Vec<u8> using crate::support::Json<Vec<u8>> {
        paginate with crate::Controller { state = default; }
    }
}}

fn main() {}
