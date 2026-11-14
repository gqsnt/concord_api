//! Preparation from the URL of an answer keeps its exact hidden signature.

use concord::__private::{Call, Prepared};

// This compile-only check must spell the exact answered-from preparation signature.
#[allow(clippy::type_complexity)]
fn prepare_next<C: Call>(
    call: C,
    response_url: concord::reqwest::Url,
) -> Result<Prepared<C::Value, C::Problem>, concord::Error<C::Problem>> {
    call.answered_from(response_url).prepare()
}

fn send_raw<C: Call>(
    prepared: Prepared<C::Value, C::Problem>,
) -> concord::Sending<concord::reqwest::Response, C::Problem> {
    prepared.send_raw()
}

fn handoff<C: Call>() {
    // Also preserve the explicit generic handoff signatures.
    let _ = prepare_next::<C>;
    let _ = send_raw::<C>;
}

#[test]
fn every_tour_call_provides_answer_context_and_raw_preparation_handoffs() {
    handoff::<tour::tasks::List>();
    handoff::<tour::tasks::Create>();
    handoff::<tour::tasks::Get>();
    handoff::<tour::tasks::Delete>();
    handoff::<tour::riot::matches::Ids>();
    handoff::<tour::riot::matches::Get>();
    handoff::<tour::riot::account::ByRiotId>();
    handoff::<tour::riot::account::Me>();
    handoff::<tour::files::Grant>();
    handoff::<tour::files::Exists>();
    handoff::<tour::files::Readme>();
    handoff::<tour::files::Download>();
    handoff::<tour::files::Upload>();
    handoff::<tour::files::Search>();
    handoff::<tour::feed::List>();
    handoff::<tour::feed::ByCursor>();
    handoff::<tour::feed::Linked>();
}

#[test]
fn raw_send_defers_preparation_after_recording_an_answer_url() {
    let url: concord::reqwest::Url = "https://example.test/a/page".parse().unwrap();
    drop(
        tour::Feed::new()
            .linked()
            .answered_from(url.clone())
            .send_raw(),
    );
    drop(
        tour::Riot::new("key")
            .account(tour::Europe)
            .me("token")
            .answered_from(url.clone())
            .send_raw(),
    );
    drop(
        tour::Files::new("key")
            .download(tour::FileId::from("file"))
            .answered_from(url)
            .send_raw(),
    );
}

#[tokio::test]
async fn c10_10_every_call_runs_the_answer_context_and_raw_handoff() {
    async fn run<C: Call>(call: C) {
        let url = "https://example.test/a/page".parse().unwrap();
        let prepared = prepare_next(call, url).unwrap_or_else(|e| panic!("{e}"));
        assert_eq!(
            send_raw::<C>(prepared)
                .await
                .unwrap_or_else(|e| panic!("{e}"))
                .status(),
            200
        );
    }
    let tasks = tour::Tasks::new().with_transport(concord::fake(|_| Ok(concord::reply(200, ""))));
    run(tasks.list()).await;
    run(tasks.get(1)).await;
    run(tasks.create(tour::NewTask {
        title: "task".into(),
    }))
    .await;
    run(tasks.delete(1)).await;
    let riot =
        tour::Riot::new("key").with_transport(concord::fake(|_| Ok(concord::reply(200, ""))));
    run(riot.matches(tour::Europe).ids(tour::Puuid::from("p"))).await;
    run(riot.matches(tour::Europe).get(tour::MatchId::from("id"))).await;
    run(riot.account(tour::Europe).by_riot_id("a", "b")).await;
    run(riot.account(tour::Europe).me("token")).await;
    let files =
        tour::Files::new("key").with_transport(concord::fake(|_| Ok(concord::reply(200, ""))));
    run(files.grant("client_credentials", "id", "secret")).await;
    run(files.exists(tour::DocId::from("doc"))).await;
    run(files.readme("repo")).await;
    run(files.download(tour::FileId::from("file"))).await;
    run(files.upload("f", concord::Body::new("bytes", "text/plain"))).await;
    run(files.search("needle")).await;
    let feed = tour::Feed::new().with_transport(concord::fake(|_| Ok(concord::reply(200, ""))));
    run(feed.list()).await;
    run(feed.by_cursor()).await;
    run(feed.linked()).await;
}
