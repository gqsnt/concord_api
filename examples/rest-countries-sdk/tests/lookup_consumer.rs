//! Consumer use of the public typed lookups, field options, and bounded walker.
//! All records and notices below are authored local samples, not live provider data.
#![allow(missing_docs)]

mod support;

use std::num::{NonZeroU32, NonZeroUsize};

use concord_example_testserver::{Recorded, Reply, TestServer};
use concord_sdk_rest_countries::concord_pagination::{PagesError, Resume};
use concord_sdk_rest_countries::{Fields, Language, Premium, Property, Region, Subregion, pages};
use serde_json::json;

use support::{assert_get, client};

const NOTICE: &str = "Unavailable on this fixture's free plan.";

#[tokio::test]
async fn callers_can_select_fields_and_page_region_subregion_and_language_results() {
    let server = TestServer::start(|request: &Recorded| -> Reply {
        let records = match request.path() {
            "/countries/v5/region/Europe" => [("France", "FR"), ("Germany", "DE")],
            "/countries/v5/subregion/Northern%20Europe" => [("Denmark", "DK"), ("Finland", "FI")],
            "/countries/v5/languages/English" => [("Canada", "CA"), ("Ireland", "IE")],
            _ => return Reply::json(500, r#"{"errors":[{"message":"Unexpected fixture path"}]}"#),
        };
        let offset: usize = request
            .query_all("offset")
            .first()
            .unwrap()
            .parse()
            .unwrap();
        let objects = records.get(offset).map_or_else(
            || json!([]),
            |(name, code)| {
                json!([{
                    "names": {"common": name}, "codes": {"alpha_2": code},
                    "leaders": [{"message": NOTICE}]
                }])
            },
        );
        let count = objects.as_array().unwrap().len();
        Reply::json(
            200,
            json!({"data": {
                "objects": objects,
                "meta": {"total": 2, "count": count, "limit": 1, "offset": offset,
                         "more": offset + count < 2}
            }})
            .to_string(),
        )
    })
    .await;
    let api = client(&server);
    let selected = Fields::new([
        Property::NAMES_COMMON,
        Property::CODES_ALPHA_2,
        Property::LEADERS,
        Property::BORDERS,
        Property::FLAG_EMOJI,
    ])
    .unwrap();
    let omitted = Fields::new([Property::FLAG_EMOJI]).unwrap();
    let lookups = [
        (
            api.region(&Region::parse("Europe").unwrap()),
            "/countries/v5/region/Europe",
            ["France", "Germany"],
        ),
        (
            api.subregion(&Subregion::parse("Northern Europe").unwrap()),
            "/countries/v5/subregion/Northern%20Europe",
            ["Denmark", "Finland"],
        ),
        (
            api.language(&Language::parse("English").unwrap()),
            "/countries/v5/languages/English",
            ["Canada", "Ireland"],
        ),
    ];

    for (lookup, path, expected_names) in lookups {
        let lookup = lookup
            .set_response_fields(selected.clone())
            .set_response_fields_omit(omitted.clone())
            .set_limit(NonZeroU32::new(1).unwrap())
            .set_offset(0u64);
        let before = server.requests().len();
        let first = lookup.clone().send().await.unwrap();
        assert_eq!(first.countries[0].common_name(), Some(expected_names[0]));
        assert_eq!(first.next_offset(), Ok(Some(1)));
        assert_eq!(
            server.requests().len(),
            before + 1,
            "send returns only one page"
        );

        let all = pages(lookup.clone())
            .max_pages(NonZeroUsize::new(2).unwrap())
            .items()
            .collect_complete()
            .await
            .unwrap();
        assert_eq!(
            all.iter()
                .filter_map(|c| c.common_name())
                .collect::<Vec<_>>(),
            expected_names
        );
        for country in &all {
            assert!(country.alpha_2().is_some());
            assert!(country.region.is_none() && country.subregion.is_none());
            assert!(
                country.borders.is_none(),
                "a selected but absent field is ordinary data"
            );
            assert!(country.flag.is_none(), "the omitted field stays absent");
            assert!(
                matches!(&country.leaders, Some(Premium::Withheld { message }) if message == NOTICE)
            );
        }
        assert_eq!(
            server.requests().len(),
            before + 3,
            "missing fields cause no extra calls"
        );

        let capped = pages(lookup.clone())
            .max_pages(NonZeroUsize::new(1).unwrap())
            .items()
            .collect_complete()
            .await
            .unwrap_err();
        assert_eq!(capped.collected.len(), 1);
        assert_eq!(capped.collected[0].common_name(), Some(expected_names[0]));
        assert!(matches!(&capped.error, PagesError::Bounded { .. }));
        assert_eq!(capped.error.resume(), Some(&Resume::At(1)));

        // An explicit starting offset also works independently of the walker.
        let last = lookup.set_offset(1u64).send().await.unwrap();
        assert_eq!(last.countries[0].common_name(), Some(expected_names[1]));
        assert_eq!(last.next_offset(), Ok(None));

        let requests = server.requests();
        assert_eq!(
            requests.len(),
            before + 5,
            "no retries or auth refresh calls"
        );
        for (request, offset) in requests[before..].iter().zip([0, 0, 1, 0, 1]) {
            let target = format!(
                "{path}?response_fields=names.common%2Ccodes.alpha_2%2Cleaders%2Cborders%2Cflag.emoji&response_fields_omit=flag.emoji&limit=1&offset={offset}"
            );
            assert_get(request, &target);
        }
    }
    assert_eq!(server.shutdown().await.open_after, 0);
}
