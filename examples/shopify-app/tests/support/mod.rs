//! Shared test support: fixtures, the loopback servers standing in for two shops and the result
//! storage host, and a small SHA-256 for the manifest check.

#![allow(dead_code)]

use concord::reqwest;
use concord_example_shopify_sdk::{AccessToken, ShopContext, ShopDomain, ShopifyContext};
use concord_example_testserver::{Recorded, Reply, TestServer};
use serde_json::Value;

/// Synthetic tokens: distinct per shop so a header leak between scopes is visible.
pub const TOKEN_ONE: &str = "shpat_synthetic_shop_one_TOKEN_0001";
pub const TOKEN_TWO: &str = "shpat_synthetic_shop_two_TOKEN_0002";

/// The storage host the signed URLs point at (mapped to its own loopback server).
pub const STORAGE_HOST: &str = "storage.example.test";

pub fn fixture(name: &str) -> String {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("fixture {path}: {e}"))
}

pub fn fixture_bytes(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("fixture {path}: {e}"))
}

/// A GraphQL answer as Shopify sends it: 200, JSON, the served version.
pub fn graphql(body: impl AsRef<[u8]>) -> Reply {
    Reply::json(200, body).header("x-shopify-api-version", "2026-10")
}

/// A GraphQL fixture, with `__STORAGE__` replaced by the storage server's origin.
pub fn graphql_fixture(name: &str, storage_port: Option<u16>) -> Reply {
    let mut text = fixture(name);
    if let Some(port) = storage_port {
        text = text.replace("__STORAGE__", &format!("http://{STORAGE_HOST}:{port}"));
    }
    graphql(text)
}

/// Which shop a request was sent to, after checking that it carries exactly that shop's token.
/// A request with another shop's token (a scope leak) gets a `401`.
pub fn shop_of(request: &Recorded) -> Result<&'static str, Reply> {
    let (shop, token) = if request.host.starts_with("shop-one.myshopify.com") {
        ("one", TOKEN_ONE)
    } else if request.host.starts_with("shop-two.myshopify.com") {
        ("two", TOKEN_TWO)
    } else {
        return Err(Reply::json(404, "{}"));
    };
    if request.header_all("x-shopify-access-token") != [token] {
        return Err(Reply::json(
            401,
            r#"{"errors":"[API] Invalid API key or access token"}"#,
        ));
    }
    Ok(shop)
}

/// The request's JSON body.
pub fn body(request: &Recorded) -> Value {
    serde_json::from_slice(&request.body).expect("a JSON request body")
}

/// `operationName` of a GraphQL request.
pub fn operation(request: &Recorded) -> String {
    body(request)["operationName"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

/// `variables.<name>` as text, if it is a string.
pub fn variable(request: &Recorded, name: &str) -> Option<String> {
    body(request)["variables"][name].as_str().map(str::to_owned)
}

/// An HTTP client mapping both shops and the storage host to their loopback servers.
pub fn http(shops: &TestServer, storage: Option<&TestServer>) -> reqwest::Client {
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .resolve("shop-one.myshopify.com", shops.addr())
        .resolve("shop-two.myshopify.com", shops.addr());
    if let Some(storage) = storage {
        builder = builder.resolve(STORAGE_HOST, storage.addr());
    }
    builder.build().unwrap()
}

/// The SDK against the shops' loopback server: fixture mode differs from live mode only by this
/// base URL and the resolver.
pub fn sdk(shops: &TestServer, storage: Option<&TestServer>) -> ShopifyContext {
    ShopifyContext::new(http(shops, storage))
        .with_base_url(format!("http://myshopify.com:{}", shops.addr().port()))
        .unwrap()
}

pub fn shop_one(sdk: &ShopifyContext) -> ShopContext {
    sdk.shop(
        ShopDomain::new("shop-one").unwrap(),
        &AccessToken::new(TOKEN_ONE).unwrap(),
    )
}

pub fn shop_two(sdk: &ShopifyContext) -> ShopContext {
    sdk.shop(
        ShopDomain::new("shop-two.myshopify.com").unwrap(),
        &AccessToken::new(TOKEN_TWO).unwrap(),
    )
}

/// SHA-256 (FIPS 180-4), lowercase hex: enough to check the manifest's fixture digests without
/// another dependency.
pub fn sha256_hex(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut message = data.to_vec();
    let bits = (data.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bits.to_be_bytes());
    for block in message.chunks(64) {
        let mut w = [0u32; 64];
        for (i, word) in block.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (slot, value) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *slot = slot.wrapping_add(value);
        }
    }
    h.iter().map(|v| format!("{v:08x}")).collect()
}
