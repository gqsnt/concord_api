// LANG-02 / §3.5: supported attributes are docs, `#[deprecated]`,
// `#[sensitive]` and `#[default]`. Unknown attributes and child-level
// `cfg`/`cfg_attr` are rejected rather than silently discarded; `sensitive`
// and `default` apply only to ordinary values.
#![allow(unused_imports)]

use concord::{Binary, Bytes, api};

api! {
    client UnknownEndpointAttributeApi {
        base "https://attributes.example.test";

        #[inline]
        GET read() -> Bytes via Binary;
    }
}

api! {
    client UnknownValueAttributeApi {
        base "https://attributes.example.test";
        values {
            #[serde(rename = "t")]
            query tenant: String;
        }
    }
}

api! {
    client ChildCfgApi {
        base "https://attributes.example.test";

        #[cfg(feature = "json")]
        GET read() -> Bytes via Binary;
    }
}

api! {
    client ChildCfgAttrApi {
        base "https://attributes.example.test";

        scope regional(#[cfg_attr(test, sensitive)] query region: String) {
            GET read() -> Bytes via Binary;
        }
    }
}

api! {
    client SensitiveBodyApi {
        base "https://attributes.example.test";

        POST write(#[sensitive] body payload: Bytes via Binary) -> Bytes via Binary;
    }
}

api! {
    client SensitiveScopeApi {
        base "https://attributes.example.test";

        #[sensitive]
        scope regional() {
            GET read() -> Bytes via Binary;
        }
    }
}

api! {
    client DefaultEndpointApi {
        base "https://attributes.example.test";

        #[default]
        GET read() -> Bytes via Binary;
    }
}

api! {
    client DeprecatedBodyApi {
        base "https://attributes.example.test";

        POST write(#[deprecated] body payload: Bytes via Binary) -> Bytes via Binary;
    }
}

api! {
    client SensitiveArgumentApi {
        base "https://attributes.example.test";
        values {
            #[sensitive(always)]
            query tenant: String;
        }
    }
}

fn main() {}
