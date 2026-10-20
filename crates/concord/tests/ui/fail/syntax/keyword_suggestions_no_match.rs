// Distant spellings and keywords unavailable to the current owner get no hint.
#![allow(unused_imports)]

use concord::{Empty, api};

api! {
    client UnrelatedKeywordApi {
        base "https://example.test/";
        unrelated;
    }
}

api! {
    client ClientOnlyKeywordApi {
        base "https://example.test/";
        GET read() -> () via Empty { bsae "https://example.test/"; }
    }
}

api! {
    client EndpointOnlyKeywordApi {
        base "https://example.test/";
        scope nested() { effcet safe; }
    }
}

api! {
    client UnrelatedEffectApi {
        base "https://example.test/";
        POST write() -> () via Empty { effect unrelated; }
    }
}

api! {
    client UnrelatedSourceApi {
        base "https://example.test/";
        credentials { credential key = unrelated; }
    }
}

api! {
    client UnrelatedKeyStatementApi {
        base "https://example.test/";
        query { unrelated "key" = "value"; }
    }
}

api! {
    client UnrelatedChallengeApi {
        base "https://example.test/";
        credentials {
            secret token;
            credential key = bearer(secret.token);
        }
        auth { bearer key challenge unrelated; }
    }
}

// A consumed modifier leaves only its payload, not another modifier.
api! {
    client ConsumedStatementModifierApi {
        base "https://example.test/";
        headers { replace appen "X-Tag" = "value"; }
    }
}

api! {
    client ConsumedQueryModifierApi {
        base "https://example.test/";
        query replace of;
    }
}

api! {
    client ConsumedHeaderModifierApi {
        base "https://example.test/";
        headers replace replac { "X-Tag" = "value"; }
    }
}

api! {
    client ConsumedAuthModifierApi {
        base "https://example.test/";
        auth replace of;
    }
}

fn main() {}
