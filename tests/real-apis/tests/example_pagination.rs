//! The pagers of the examples on the wire: what each sends for its next page, and where it ends.
#![allow(clippy::unwrap_used, clippy::indexing_slicing)]

use concord::error::Kind;
use concord_testkit::Script;
use real_apis::elasticsearch::{Elasticsearch, Pit};
use serde_json::json;

fn es(script: &Script) -> Elasticsearch {
    Elasticsearch::new(
        "https://es.test".parse::<concord::Base>().unwrap(),
        "ApiKey k",
    )
    .with_transport(script.clone())
}

fn hits(pit: &str, sort: Option<serde_json::Value>) -> String {
    hits_with_state(Some(pit), sort, false)
}

fn hits_with_state(pit: Option<&str>, sort: Option<serde_json::Value>, timed_out: bool) -> String {
    let hit = match sort {
        Some(sort) => json!({ "_id": "first", "sort": sort }),
        None => json!({ "_id": "first" }),
    };
    let hits = if timed_out { vec![] } else { vec![hit] };
    json!({ "timed_out": timed_out, "_shards": { "failed": 0 }, "pit_id": pit, "hits": { "hits": hits } })
        .to_string()
}

/// A page of hits whose last hit has no sort values, or the sort values that the call just sent,
/// would ask for the same hits again (a point in time whose id changes makes each request differ):
/// the pages end with a Mismatch, not with a request that repeats a hit.
#[tokio::test(start_paused = true)]
async fn elasticsearch_a_point_in_time_that_does_not_move_is_a_mismatch() {
    for (sort, reason) in [
        (None, "a last hit with no sort values"),
        (
            Some(json!([7])),
            "the last hit repeats the request's sort values",
        ),
    ] {
        let script = Script::new();
        script.answer(200).body(hits("p2", sort.clone()));
        script.answer(200).body(hits("p3", sort.clone()));
        script.answer(200).body(hits("p4", sort.clone()));
        let pit = Pit {
            id: "p1".into(),
            keep_alive: "5m".into(),
        };
        let mut call = es(&script).search_pit(json!({}), pit);
        if sort.is_some() {
            call = call.search_after(vec![json!(7)]);
        }
        let mut pages = call.pages();
        assert!(pages.try_next().await.unwrap().is_some());
        let error = pages.try_next().await.unwrap_err();
        assert_eq!(
            (error.kind(), error.reason()),
            (Kind::Mismatch, Some(reason))
        );
        assert_eq!(script.requests().len(), 1, "{reason}");
    }

    for (body, reason) in [
        (
            hits_with_state(Some("p2"), None, true),
            "a page with missing results",
        ),
        (hits_with_state(None, Some(json!([7])), false), "no pit_id"),
    ] {
        let script = Script::new();
        script.answer(200).body(body);
        let pit = Pit {
            id: "p1".into(),
            keep_alive: "5m".into(),
        };
        let mut pages = es(&script).search_pit(json!({}), pit).pages();
        assert!(pages.try_next().await.unwrap().is_some());
        assert_eq!(pages.try_next().await.unwrap_err().reason(), Some(reason));
        assert!(pages.try_next().await.unwrap().is_none());
        assert_eq!(script.requests().len(), 1);
    }
}
