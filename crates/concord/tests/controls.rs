//! `controls { .. }` (H06): an SDK withholds a generated control from its callers. A crate-visible
//! control stays callable by the declaring crate (here, from another module of this test crate);
//! a removed one is not generated (`tests/ui/controls_removed.rs`); every other control, and how
//! each request is built, is unchanged.

use concord::reqwest;

mod sdk {
    use concord::reqwest;

    concord::api! {
        /// One account: its subdomain and token belong together.
        pub client Account(
            subdomain: String,
            #[sensitive] header("x-token") token: String,
            header("x-trace") trace?: String,
        ) {
            base "https://example.com";
            controls { pub(crate) new; pub(crate) with_base_url; remove set_subdomain; }
            subdomain "{subdomain}";

            /// One shop and its own key.
            scope shop(shop: String, #[sensitive] header("x-shop-key") key: String) {
                controls { remove set_shop; }
                path "shops/{shop}";
                GET orders(query page?: u32) { path "orders"; }
            }
        }
    }

    /// The SDK's constructor: the only way to an `Account` outside this crate.
    pub fn connect(http: reqwest::Client, subdomain: &str, token: &str) -> Account {
        Account::new(
            http,
            AccountConfig {
                subdomain: subdomain.into(),
                token: token.into(),
            },
        )
    }
}

fn url(prepared: &concord::Prepared<()>) -> String {
    prepared.request().url().to_string()
}

fn header(prepared: &concord::Prepared<()>, name: &str) -> Option<String> {
    prepared
        .request()
        .headers()
        .get(name)
        .map(|v| v.to_str().unwrap().to_owned())
}

#[test]
fn kept_controls_and_requests_are_unchanged() -> Result<(), concord::Error> {
    let account = sdk::connect(reqwest::Client::new(), "acme", "t-1");
    // The controls not listed stay public, with their ordinary signatures.
    let _: fn(sdk::Account, String) -> sdk::Account = sdk::Account::set_token;
    let _: fn(sdk::Account, String) -> sdk::Account = sdk::Account::set_trace;
    let _: fn(sdk::Account, Option<String>) -> sdk::Account = sdk::Account::maybe_trace;
    let _: fn(sdk::AccountShop, String) -> sdk::AccountShop = sdk::AccountShop::set_key;

    let orders = account.shop("s1", "k-1").orders().set_page(2).prepare()?;
    assert_eq!(
        url(&orders),
        "https://acme.example.com/shops/s1/orders?page=2"
    );
    assert_eq!(header(&orders, "x-token").as_deref(), Some("t-1"));
    assert_eq!(header(&orders, "x-shop-key").as_deref(), Some("k-1"));

    // Rotation: same account, same shop, another token or key.
    let rotated = account.clone().set_token("t-2").set_trace("on");
    let orders = rotated
        .shop("s1", "k-1")
        .set_key("k-2")
        .orders()
        .prepare()?;
    assert_eq!(url(&orders), "https://acme.example.com/shops/s1/orders");
    assert_eq!(header(&orders, "x-token").as_deref(), Some("t-2"));
    assert_eq!(header(&orders, "x-shop-key").as_deref(), Some("k-2"));
    assert_eq!(header(&orders, "x-trace").as_deref(), Some("on"));

    // Another account or shop is another entry, with its own credentials.
    let other = sdk::connect(reqwest::Client::new(), "beta", "t-9");
    let orders = other.shop("s2", "k-9").orders().prepare()?;
    assert_eq!(url(&orders), "https://beta.example.com/shops/s2/orders");
    assert_eq!(header(&orders, "x-token").as_deref(), Some("t-9"));
    Ok(())
}

#[test]
fn crate_visible_controls_are_callable_by_the_declaring_crate() -> Result<(), concord::Error> {
    // `pub(crate) new;` and `pub(crate) with_base_url;`: this test crate declared the client.
    let account = sdk::Account::new(
        reqwest::Client::new(),
        sdk::AccountConfig {
            subdomain: "acme".into(),
            token: "t-1".into(),
        },
    )
    .with_base_url("http://example.test:8080/v2")?;
    let orders = account.shop("s1", "k-1").orders().prepare()?;
    assert_eq!(
        url(&orders),
        "http://acme.example.test:8080/v2/shops/s1/orders"
    );
    // The old handle keeps the old base, as with a public `with_base_url`.
    let moved = sdk::connect(reqwest::Client::new(), "acme", "t-1");
    let before = moved.shop("s1", "k-1");
    let _moved = moved.with_base_url("http://example.test:9090")?;
    assert_eq!(
        url(&before.orders().prepare()?),
        "https://acme.example.com/shops/s1/orders"
    );
    Ok(())
}
