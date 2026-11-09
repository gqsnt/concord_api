//! The SDK error variants a caller matches, after `derive(concord::Error)` (needs `DISCORD_BOT_TOKEN`, `DISCORD_CHANNEL_ID`).

use std::time::Duration;

use concord_sdk_discord_rest::concord::reqwest;
use concord_sdk_discord_rest::{
    ChannelId, Discord, DiscordConfig, Error, MessageLimit, RateLimiter,
};

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
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
    let channel = ChannelId::new(std::env::var("DISCORD_CHANNEL_ID")?.parse()?);

    match discord
        .channel(channel)
        .messages()
        .set_limit(MessageLimit::MAX)
        .send()
        .await
    {
        Ok(messages) => println!("{} messages", messages.len()),
        Err(error) => {
            eprintln!(
                "operation {:?}; status {:?}; resource {:?}",
                error.operation(),
                error.status(),
                error.resource()
            );
            // These exact tuple and struct patterns are the existing caller contract.
            match &error {
                Error::Gated(gate) => {
                    eprintln!(
                        "Nothing sent; try deliberately after {:?}",
                        gate.retry_after
                    );
                }
                Error::RateLimited(limit) => {
                    eprintln!("429 {:?}; delay {:?}", limit.scope, limit.retry_after);
                }
                Error::Api(api) => {
                    eprintln!("HTTP {}; Discord code {:?}", api.status(), api.code());
                }
                Error::AmbiguousWrite(write) => {
                    eprintln!("Reconcile the write; accepted status {:?}", write.accepted);
                }
                Error::Protocol(protocol) => eprintln!("Provider rule {:?}", protocol.problem),
                Error::Concord {
                    error: cause,
                    resource,
                } => {
                    eprintln!("{:?} on {resource:?}", cause.kind());
                }
            }
            return Err(error.into());
        }
    }
    Ok(())
}
