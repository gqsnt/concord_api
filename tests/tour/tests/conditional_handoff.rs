//! Conditional preparation and sending, including the hidden handoff.

use concord::__private::{Call, Prepared};

// This compile-only check must spell the exact conditional handoff signatures.
#[allow(clippy::type_complexity)]
async fn handoff<C: Call>(
    call: C,
    prepare: fn(
        C,
        &str,
    ) -> Result<Prepared<Option<C::Value>, C::Problem>, concord::Error<C::Problem>>,
    send: fn(C, &str) -> concord::Sending<concord::Answer<Option<C::Value>>, C::Problem>,
) -> Result<(), concord::Error<C::Problem>> {
    let prepared = prepare(call.clone(), "etag")?;
    assert_eq!(
        prepared
            .request
            .headers()
            .get("if-none-match")
            .map(|v| v.as_bytes()),
        Some(b"etag".as_slice())
    );
    assert!(prepared.send().await?.value().is_none());
    assert!(send(call, "etag").await?.value().is_none());
    Ok(())
}

#[tokio::test]
async fn every_get_and_head_provides_the_conditional_handoff(
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let tasks = tour::Tasks::new().with_transport(concord::fake(|_| Ok(concord::reply(304, ""))));
    let riot =
        tour::Riot::new("key").with_transport(concord::fake(|_| Ok(concord::reply(304, ""))));
    let files =
        tour::Files::new("key").with_transport(concord::fake(|_| Ok(concord::reply(304, ""))));
    let feed = tour::Feed::new().with_transport(concord::fake(|_| Ok(concord::reply(304, ""))));
    handoff(
        tasks.list(),
        tour::tasks::List::__prepare_if_none_match,
        tour::tasks::List::send_if_none_match,
    )
    .await?;
    handoff(
        tasks.get(1),
        tour::tasks::Get::__prepare_if_none_match,
        tour::tasks::Get::send_if_none_match,
    )
    .await?;
    handoff(
        riot.matches(tour::Europe).ids(tour::Puuid::from("p")),
        tour::riot::matches::Ids::__prepare_if_none_match,
        tour::riot::matches::Ids::send_if_none_match,
    )
    .await?;
    handoff(
        riot.matches(tour::Europe).get(tour::MatchId::from("id")),
        tour::riot::matches::Get::__prepare_if_none_match,
        tour::riot::matches::Get::send_if_none_match,
    )
    .await?;
    handoff(
        riot.account(tour::Europe).by_riot_id("a", "b"),
        tour::riot::account::ByRiotId::__prepare_if_none_match,
        tour::riot::account::ByRiotId::send_if_none_match,
    )
    .await?;
    handoff(
        riot.account(tour::Europe).me("token"),
        tour::riot::account::Me::__prepare_if_none_match,
        tour::riot::account::Me::send_if_none_match,
    )
    .await?;
    handoff(
        files.exists(tour::DocId::from("doc")),
        tour::files::Exists::__prepare_if_none_match,
        tour::files::Exists::send_if_none_match,
    )
    .await?;
    handoff(
        files.readme("repo"),
        tour::files::Readme::__prepare_if_none_match,
        tour::files::Readme::send_if_none_match,
    )
    .await?;
    handoff(
        files.download(tour::FileId::from("file")),
        tour::files::Download::__prepare_if_none_match,
        tour::files::Download::send_if_none_match,
    )
    .await?;
    handoff(
        feed.list(),
        tour::feed::List::__prepare_if_none_match,
        tour::feed::List::send_if_none_match,
    )
    .await?;
    handoff(
        feed.by_cursor(),
        tour::feed::ByCursor::__prepare_if_none_match,
        tour::feed::ByCursor::send_if_none_match,
    )
    .await?;
    handoff(
        feed.linked(),
        tour::feed::Linked::__prepare_if_none_match,
        tour::feed::Linked::send_if_none_match,
    )
    .await?;
    Ok(())
}

#[test]
fn conditional_send_defers_preparation_and_does_not_borrow_the_etag() {
    let sending = {
        let etag = String::from("etag");
        tour::Tasks::new().get(1).send_if_none_match(&etag)
    };
    // Construction must not prepare or borrow the caller's ETag.
    drop(sending);
}
