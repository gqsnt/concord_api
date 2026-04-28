use concord_macros::api;

api! {
    client OldClientRootSyntax {
        scheme: https,
        host: "example.com"
    }
}

fn main() {}
