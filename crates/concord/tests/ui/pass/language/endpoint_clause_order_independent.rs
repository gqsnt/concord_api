// §3.3: endpoint-local clause order across different families does not imply
// execution order; a rate attachment may precede the timeout selection.
use concord::{Binary, Bytes, api};

api! {
    client Api {
        timeout 30s;
        base "https://example.test";

        GET read() -> Bytes via Binary {
            rate_limit { id "concord.endpoint_clause_order_independent.1"; key [origin]; cost 1; allow 1 per 1s; }
            timeout 1s;
        }
    }
}

fn main() {}
