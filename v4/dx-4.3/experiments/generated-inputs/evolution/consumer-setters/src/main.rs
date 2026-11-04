//! A downstream consumer saved against SDK v1 that uses only setters and navigation (the V4.2
//! style). Never edited afterwards.
use g1_evo_sdk::{Shop, ShopConfig};

fn main() {
    let http = concord::reqwest::Client::new();
    let shop = Shop::new(http, ShopConfig { key: "k".into() });
    let listing = shop.listing().set_default_tags(vec!["featured".to_owned()]);
    let request = listing.list().set_q("lamp").set_limit(5).set_tags(Vec::new());
    let prepared = request.prepare().expect("builds");
    println!("{}", prepared.request().url());
}
