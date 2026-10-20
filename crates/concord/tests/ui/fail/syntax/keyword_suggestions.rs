// Spelling hints retain the owning declaration, code and offending token span.
#![allow(unused_imports)]

use concord::{Empty, api};

api! {
    cleint RootKeywordApi { base "https://example.test/"; }
}

api! {
    client ClientKeywordApi {
        bas "https://example.test/";
    }
}

api! {
    client ScopeKeywordApi {
        base "https://example.test/";
        scope nested() {
            respnose Empty;
        }
    }
}

api! {
    client EndpointKeywordApi {
        base "https://example.test/";
        GET read() -> () via Empty {
            headars { "X-Tag" = "tag"; }
        }
    }
}

api! {
    client MethodKeywordApi {
        base "https://example.test/";
        GTE read() -> () via Empty;
    }
}

api! {
    client ReplaceKeywordApi {
        base "https://example.test/";
        query { replaec "key" = "value"; }
    }
}

api! {
    client WhenPresentKeywordApi {
        base "https://example.test/";
        query { when_presnt "key" = "value"; }
    }
}

api! {
    client AppendKeywordApi {
        base "https://example.test/";
        query { appand "key" = "value"; }
    }
}

api! {
    client RemoveKeywordApi {
        base "https://example.test/";
        headers { removee "X-Tag"; }
    }
}

api! {
    client EffectKeywordApi {
        base "https://example.test/";
        POST write() -> () via Empty { effect idempotnet; }
    }
}

api! {
    client SafeEffectKeywordApi {
        base "https://example.test/";
        GET read() -> () via Empty { effect sfe; }
    }
}

api! {
    client NonIdempotentEffectKeywordApi {
        base "https://example.test/";
        POST write() -> () via Empty { effect non_idempotnet; }
    }
}

api! {
    client ApiKeySourceKeywordApi {
        base "https://example.test/";
        credentials {
            secret token;
            credential key = api_kye(secret.token);
        }
    }
}

api! {
    client BearerSourceKeywordApi {
        base "https://example.test/";
        credentials {
            secret token;
            credential key = beare(secret.token);
        }
    }
}

api! {
    client BasicSourceKeywordApi {
        base "https://example.test/";
        credentials {
            secret token;
            credential key = bsaic(secret.token, secret.token);
        }
    }
}

api! {
    client EndpointSourceKeywordApi {
        base "https://example.test/";
        credentials { credential key = endpoin fetch; }
    }
}

api! {
    client CustomSourceKeywordApi {
        base "https://example.test/";
        credentials { credential key = custmo Provider; }
    }
}

api! {
    client ChallengeKeywordApi {
        base "https://example.test/";
        credentials {
            secret token;
            credential key = bearer(secret.token);
        }
        auth { bearer key challenge unauthroized; }
    }
}

api! {
    client NeverChallengeKeywordApi {
        base "https://example.test/";
        credentials {
            secret token;
            credential key = bearer(secret.token);
        }
        auth { bearer key challenge nevre; }
    }
}

api! {
    client ForbiddenChallengeKeywordApi {
        base "https://example.test/";
        credentials {
            secret token;
            credential key = bearer(secret.token);
        }
        auth { bearer key challenge unauthorized_or_forbidde; }
    }
}

fn main() {}
