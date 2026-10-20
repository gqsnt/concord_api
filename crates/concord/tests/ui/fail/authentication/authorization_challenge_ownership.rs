// AUTH-01 / §10.1 (replaces the stale v2 `authorization_custom_requires_challenge`
// fixture, which failed only because its provider type did not exist): at most
// one effective placement attributes a non-`never` challenge, and a challenge
// status is disjoint from accepted payload and expected-absence statuses.
#![allow(unused_imports)]

use concord::{Binary, Bytes, Text, api};

api! {
    client TwoChallengeOwnersApi {
        base "https://example.test";
        credentials {
            secret key;
            secret token;
            credential key = api_key(secret.key);
            credential token = bearer(secret.token);
        }
        auth { bearer token challenge unauthorized; }

        GET protected() -> Bytes via Binary {
            auth { header "X-Key" = key challenge unauthorized_or_forbidden; }
        }
    }
}

api! {
    client AcceptedChallengeApi {
        base "https://example.test";
        credentials {
            secret token;
            credential token = bearer(secret.token);
        }
        auth { bearer token challenge unauthorized; }

        GET protected() -> Bytes via Binary {
            status [200, 401];
        }
    }
}

api! {
    client AbsentChallengeApi {
        base "https://example.test";
        credentials {
            secret token;
            credential token = bearer(secret.token);
        }
        auth { bearer token challenge unauthorized_or_forbidden; }

        GET find() -> Option<String> via Text {
            absent on 403;
        }
    }
}

fn main() {}
