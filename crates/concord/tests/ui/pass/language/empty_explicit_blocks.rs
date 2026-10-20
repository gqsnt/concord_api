// LANG-02 (§3.2) / PATCH-01 (§6.1): an empty explicit query/header block is
// legal. An empty `query { }` / `headers { }` contributes no statement and
// keeps the inherited program; an empty `query replace { }` /
// `headers replace { }` is a legal empty family replacement that clears the
// inherited emissions. Plans show exactly that, with no invented entries.
use concord::http::HeaderName;
use concord::{Binary, Bytes, api};

api! {
    client EmptyBlocksApi {
        base "http://empty-blocks.example.test";
        query { "tenant" = "acme"; }
        headers { "X-Trace" = "root"; }

        scope kept() {
            query { }
            headers { }

            GET read() -> Bytes via Binary {
                path "kept";
                query { }
                headers { }
            }
        }

        scope cleared() {
            query replace { }
            headers replace { }

            GET read() -> Bytes via Binary {
                path "cleared";
            }
        }

        GET local() -> Bytes via Binary {
            path "local";
            query replace { }
            headers replace { }
        }
    }
}

fn main() {
    let trace = HeaderName::from_static("x-trace");
    let client = EmptyBlocksApi::new().expect("client");

    let kept = client.kept().read().plan().expect("plan");
    assert_eq!(kept.observed_url(), "http://empty-blocks.example.test/kept?tenant=acme");
    assert!(kept.public_headers().contains_key(&trace));

    // Only the authored header disappears: representation negotiation
    // headers are unaffected, and nothing is invented for an empty block.
    let cleared = client.cleared().read().plan().expect("plan");
    assert_eq!(cleared.observed_url(), "http://empty-blocks.example.test/cleared");
    assert!(!cleared.public_headers().contains_key(&trace));
    assert_eq!(cleared.public_headers().len(), kept.public_headers().len() - 1);

    let local = client.local().plan().expect("plan");
    assert_eq!(local.observed_url(), "http://empty-blocks.example.test/local");
    assert!(!local.public_headers().contains_key(&trace));
    assert_eq!(local.public_headers().len(), cleared.public_headers().len());
}
