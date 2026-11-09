//! One call, one automatic Concord event; no request value is passed to a logging macro.

use concord::reqwest;
use concord_sdk_nager_holidays::NagerHolidays;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_max_level(tracing_subscriber::filter::LevelFilter::INFO)
        .without_time()
        .with_ansi(false)
        .init();
    let api = NagerHolidays::new(reqwest::Client::new());
    let _holidays = api.holidays("AT", 2026).send().await?;
    Ok(())
}
