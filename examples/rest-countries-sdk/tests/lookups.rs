//! Regression checks for typed property lookups, using only loopback fixtures.
#![allow(missing_docs)]

mod support;

use std::num::NonZeroUsize;

use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_rest_countries::{InputError, Language, Region, Subregion, pages};

use support::{assert_get, client, json};

#[tokio::test]
async fn lookup_values_are_validated_before_a_request_can_be_sent() {
    let server = TestServer::start(|_: &Recorded| -> Reply {
        json(200, fixture!("synthetic-no-match.json"))
    })
    .await;
    let api = client(&server);

    for invalid in [
        "",
        " ",
        " Europe",
        "Europe ",
        "a\nb",
        "a\tb",
        "a\0b",
        "a\u{7f}b",
        "a\u{85}b",
        "a\u{2028}b",
        ".",
        "..",
        "a/b",
        "a\\b",
        "a?b",
        "a#b",
        "Northern%20Europe",
        "a+b",
    ] {
        assert_eq!(
            Region::parse(invalid),
            Err(InputError::Region),
            "{invalid:?}"
        );
        assert_eq!(
            Subregion::parse(invalid),
            Err(InputError::Subregion),
            "{invalid:?}"
        );
        assert_eq!(
            Language::parse(invalid),
            Err(InputError::Language),
            "{invalid:?}"
        );
    }

    let region = Region::parse("europe").unwrap();
    let subregion = Subregion::parse("Northern Europe").unwrap();
    assert_eq!(region.as_str(), "europe");
    assert_eq!(subregion.as_str(), "Northern Europe");
    assert_eq!(region.to_string(), "europe");
    assert_eq!(subregion.to_string(), "Northern Europe");
    for value in [
        "English",
        "eng",
        "en",
        "pt-BR",
        "Français",
        "Norwegian (Bokmål)",
    ] {
        let language = Language::parse(value).unwrap();
        assert_eq!(language.as_str(), value);
        assert_eq!(language.to_string(), value);
        let _request = api.language(&language);
    }
    let _region = api.region(&region);
    let _subregion = api.subregion(&subregion);
    assert!(
        server.requests().is_empty(),
        "validation and construction send nothing"
    );
    assert_eq!(server.shutdown().await.open_after, 0);
}

#[tokio::test]
async fn unknown_lookup_values_are_empty_pages_and_end_the_walk_after_one_request() {
    let server = TestServer::start(|_: &Recorded| -> Reply {
        json(200, fixture!("synthetic-no-match.json"))
    })
    .await;
    let api = client(&server);
    let lookups = [
        (
            api.region(&Region::parse("Atlantis").unwrap()),
            "/countries/v5/region/Atlantis",
        ),
        (
            api.subregion(&Subregion::parse("Northern Atlantis").unwrap()),
            "/countries/v5/subregion/Northern%20Atlantis",
        ),
        (
            api.language(&Language::parse("Unmatched language").unwrap()),
            "/countries/v5/languages/Unmatched%20language",
        ),
    ];

    for (lookup, target) in lookups {
        let before = server.requests().len();
        let page = lookup.clone().send().await.unwrap();
        assert!(page.countries.is_empty());
        assert_eq!(
            (page.meta.total, page.meta.count, page.meta.more),
            (0, 0, false)
        );
        assert_eq!(page.next_offset(), Ok(None));
        assert_eq!(server.requests().len(), before + 1);

        let all = pages(lookup)
            .max_pages(NonZeroUsize::new(3).unwrap())
            .items()
            .collect_complete()
            .await
            .unwrap();
        assert!(all.is_empty());
        let requests = server.requests();
        assert_eq!(requests.len(), before + 2, "no follow-up page or auth work");
        assert_get(&requests[before], target);
        assert_get(&requests[before + 1], target);
    }
    assert_eq!(server.shutdown().await.open_after, 0);
}
