//! List tagged droplets; a provider continuation must retain the exact tag filter.
use concord_sdk_digitalocean::{DigitalOcean, PageSize, TagName};
use std::num::NonZeroUsize;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api = DigitalOcean::connect(&std::env::var("DIGITALOCEAN_TOKEN")?)?;
    let tag = TagName::new("env:prod")?;
    let mut pages =
        api.tagged_droplet_pages(&tag, PageSize::new(100)?, NonZeroUsize::new(20).unwrap());
    while let Some(page) = pages.next_page().await? {
        // First and followed pages both used Policy, retaining quota/error metadata.
        let _remaining = page.meta.remaining()?;
        for droplet in page.items {
            println!("{} {}", droplet.id, droplet.name);
        }
    }
    Ok(())
}
