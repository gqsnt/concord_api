//! Shared live-mode setup of the two binaries (included with `#[path]`, not a module of the
//! library): credentials from the environment, or `None` (the binary then prints `NOT_RUN`).

use concord::reqwest;
use concord_example_shopify_sdk::{AccessToken, ShopContext, ShopDomain, ShopifyContext};

/// `SHOPIFY_SHOP` (`acme` or `acme.myshopify.com`) and `SHOPIFY_ADMIN_TOKEN`, both required.
pub fn shop_from_env() -> Result<Option<ShopContext>, Box<dyn std::error::Error>> {
    let (Ok(shop), Ok(token)) = (
        std::env::var("SHOPIFY_SHOP"),
        std::env::var("SHOPIFY_ADMIN_TOKEN"),
    ) else {
        return Ok(None);
    };
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(std::time::Duration::from_secs(60))
        .build()?;
    let sdk = ShopifyContext::new(http);
    Ok(Some(
        sdk.shop(ShopDomain::new(&shop)?, &AccessToken::new(token)?),
    ))
}
