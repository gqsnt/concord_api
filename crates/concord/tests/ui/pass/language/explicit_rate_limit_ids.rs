// §12.2 / §12.3: one explicit rate policy ID defined inline under two
// unrelated scopes shares one identity/state when the key schema and window
// set are identical; per-call cost differs and is not a bucket dimension.
// v3 inline policies begin with `id` (fixed clause order), and the `endpoint`
// dimension requires every covered endpoint to declare an explicit ID
// (IDENTITY-01).
use concord::{Binary, Bytes, api};

api! {
    client Api {
        timeout 30s;
        base "https://example.test";

        scope first() {
            rate_limit {
                id "service.shared.quota";
                key [origin, endpoint];
                cost 1;
                allow 10 per 1s;
                allow 100 per 1m;
            }
            GET read() -> Bytes via Binary {
                id "service.first.read";
            }
        }

        scope renamed() {
            rate_limit {
                id "service.shared.quota";
                key [origin, endpoint];
                cost 2;
                allow 100 per 1m;
                allow 10 per 1s;
            }
            GET write() -> Bytes via Binary {
                id "service.renamed.write";
            }
        }
    }
}

fn main() {}
