//! Walk before a message ID, with the same page validation and rate admission as a fresh walk.
use std::num::NonZeroUsize;

use concord_sdk_discord_rest::concord::reqwest;
use concord_sdk_discord_rest::pagination::Resume;
use concord_sdk_discord_rest::{
    ChannelId, Discord, DiscordConfig, MessageId, MessageLimit, RateLimiter,
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .build()?;
    let discord = Discord::new(
        http,
        DiscordConfig {
            token: std::env::var("DISCORD_BOT_TOKEN")?,
            limiter: RateLimiter::new(),
        },
    );
    let channel = discord.channel(ChannelId::new(41771983423143937));
    let from = match std::env::args().nth(1) {
        Some(id) => Resume::At(MessageId::new(id.parse()?)),
        None => Resume::Start,
    };
    // Resume::At(id) requests messages before id; a failed page's Resume works here too.
    let mut pages = channel
        .history(MessageLimit::new(50)?)
        .starting_at(from)
        .max_pages(NonZeroUsize::new(20).unwrap());
    while let Some(page) = pages.next_page().await? {
        for message in page.items {
            println!("{}", message.id);
        }
    }
    Ok(())
}
