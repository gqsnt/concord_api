use concord::api;

api! {
    client PlanApi {
        base "https://example.test";
        timeout 30s;
        GET plan() -> Bytes via Binary;
    }
}

api! {
    client ContractApi {
        base "https://example.test";
        timeout 30s;
        GET contract() -> Bytes via Binary;
    }
}

api! {
    client UnboundApi {
        base "https://example.test";
        timeout 30s;
        scope unbound() {
            GET read() -> Bytes via Binary;
        }
    }
}

fn main() {}
