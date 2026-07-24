#[cfg(feature = "fixture-absence")]
use runtime::auth::OAuth2ClientCredentials;

fn main() {
    #[cfg(feature = "fixture-absence")]
    let _: Option<OAuth2ClientCredentials> = None;
}
