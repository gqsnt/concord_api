use concord::api;

api! {
    client PlanApi {
        base "https://example.test";
        timeout off;
        GET plan() -> Bytes using Bytes;
    }
}

api! {
    client ContractApi {
        base "https://example.test";
        timeout off;
        GET contract() -> Bytes using Bytes;
    }
}

api! {
    client UnboundApi {
        base "https://example.test";
        timeout off;
        scope unbound() {
            GET read() -> Bytes using Bytes;
        }
    }
}

fn main() {}
