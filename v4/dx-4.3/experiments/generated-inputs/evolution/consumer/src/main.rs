//! A downstream consumer saved against SDK v1, using the documented construction style of the
//! generated inputs: `new()`, field assignment, `list_with`, `input()`. Never edited afterwards.
use g1_evo_sdk::{Shop, ShopConfig, ShopListingListInput};

fn main() {
    let http = concord::reqwest::Client::new();
    let shop = Shop::new(http, ShopConfig { key: "k".into() });
    let listing = shop.listing().set_default_tags(vec!["featured".to_owned()]);

    let mut saved = ShopListingListInput::new();
    saved.q = Some("lamp".to_owned());
    saved.limit = 5;
    let mut unfiltered = saved.clone();
    unfiltered.tags = Some(Vec::new());

    let request = listing.list_with(unfiltered);
    assert_eq!(request.input().limit, 5);
    let prepared = request.prepare().expect("builds");
    println!("{}", prepared.request().url());
}
