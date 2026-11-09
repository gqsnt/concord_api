// Shared test binaries use different subsets of this fixture catalog.
#![allow(dead_code)]

use std::time::Duration;

use concord_sdk_mediawiki_action_api::{MediaWiki, MediaWikiConfig, concord::reqwest};
use concord_testserver::{Recorded, Reply};

pub const SEARCH: &str = include_str!("../../fixtures/search-first.json");
pub const SEARCH_LAST: &str = include_str!("../../fixtures/search-last.json");
pub const CONTENT: &str = include_str!("../../fixtures/content.json");
pub const CATEGORY_FIRST: &str = include_str!("../../fixtures/category-first.json");
pub const CATEGORY_EMPTY: &str = include_str!("../../fixtures/category-empty.json");
pub const CATEGORY_LAST: &str = include_str!("../../fixtures/category-last.json");
pub const API_ERROR: &str = include_str!("../../fixtures/api-error.json");
pub const UA: &str = "SDK39-Fixture/1.0 (contact: fixture@example.invalid)";

pub fn api(url: &str) -> MediaWiki {
    api_with_user_agent(url, UA)
}

pub fn api_with_user_agent(url: &str, user_agent: &str) -> MediaWiki {
    concord::BaseUrl::parse((format!("{url}/w/api.php")).as_ref())
        .map(|origin| {
            MediaWiki::new(
                reqwest::Client::builder()
                    .no_proxy()
                    .redirect(reqwest::redirect::Policy::none())
                    .retry(reqwest::retry::never())
                    .timeout(Duration::from_secs(5))
                    .build()
                    .unwrap(),
                MediaWikiConfig {
                    origin,
                    user_agent: user_agent.into(),
                },
            )
        })
        .unwrap()
}

pub fn param(request: &Recorded, name: &str) -> Option<String> {
    request
        .query_pairs()
        .into_iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value)
}

pub const BATCH_FIRST: &str = include_str!("../../fixtures/batch-first.json");
pub const BATCH_LAST: &str = include_str!("../../fixtures/batch-last.json");
/// The distinct wire value for the batch fixtures' requested IDs `[12, 404, 7, 12, 31, 5, 8]`.
pub const BATCH_WIRE_IDS: &str = "12|404|7|31|5|8";

// Synthetic batch oracle (API:Continue: a continued prop query lists the same pages again with
// the next properties). Only the whole continuation reaches the last response; anything else
// replays the first response, as a server restarting the batch would.
pub fn batch_service(request: &Recorded) -> Reply {
    assert_eq!(param(request, "pageids").as_deref(), Some(BATCH_WIRE_IDS));
    match (param(request, "rvcontinue"), param(request, "continue")) {
        (Some(rv), Some(generic)) if rv == "7|9001" && generic == "||" => {
            Reply::json(200, BATCH_LAST)
        }
        _ => Reply::json(200, BATCH_FIRST),
    }
}

// Synthetic oracle authored from the capsule and the official continuation examples.
// Missing *any* field replays the first page. This is a controlled failure scenario,
// not an assertion that all deployed MediaWiki versions behave identically.
pub fn category_service(request: &Recorded) -> Reply {
    let body = match (
        param(request, "cmcontinue"),
        param(request, "continue"),
        param(request, "futureoffset"),
    ) {
        (Some(cm), Some(generic), Some(extra))
            if cm == "page|café & /+|2" && generic == "-||" && extra == "7" =>
        {
            CATEGORY_EMPTY
        }
        (Some(cm), Some(generic), Some(extra))
            if cm == "page|z|3" && generic == "||" && extra == "8" =>
        {
            CATEGORY_LAST
        }
        _ => CATEGORY_FIRST,
    };
    Reply::json(200, body)
}
