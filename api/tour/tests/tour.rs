//! The tour examples and endpoint surfaces, exercised over in-memory transports.
//! Pagination examples run through Script without a socket.
#![allow(dead_code, unused_variables)]

use concord::{Retry, Secret};
use std::future::IntoFuture;
use std::time::Duration;
use tour::*;

type AnyError = Box<dyn std::error::Error + Send + Sync>;

// Tour 1: the complete client-call block, with application types supplied above.
async fn section_1(tasks: Tasks) -> tasks::Result<()> {
    let open = tasks.list().done(false).await?;
    let task = tasks
        .create(NewTask {
            title: "Write the docs".into(),
        })
        .await?;
    tasks.delete(task.id).await?;
    Ok(())
}

// Tour 3: a temporary scope handle must not lend its lifetime to the call.
// Keep the compile-only application example's indexing as written in the tour.
#[allow(clippy::indexing_slicing)]
async fn section_3(riot: &Riot, puuid: Puuid) -> riot::Result<()> {
    let ids = riot.matches(Europe).ids(&puuid).count(5).send().await?;
    let game = riot.matches(Europe).get(&ids[0]).await?;
    Ok(())
}

// Tour 4: a 404 is an ordinary None, while a 2xx still reads its JSON body.
// This compile-only application example shows how a caller prints a result.
#[allow(clippy::print_stdout)]
async fn section_4(riot: &Riot) -> riot::Result<()> {
    match riot.account(Europe).by_riot_id("Faker", "KR1").await? {
        Some(account) => println!("{}", account.puuid),
        None => println!("nobody by that name"),
    }
    Ok(())
}

// Tour 5: Items has its own fallible, asynchronous next method.
// This application example also runs below with scripted pages.
#[allow(clippy::print_stdout)]
async fn section_5(riot: &Riot, puuid: Puuid) -> riot::Result<()> {
    let mut ids = riot.matches(Europe).ids(&puuid).pages().items();
    while let Some(id) = ids.try_next().await? {
        println!("{id}");
    }
    Ok(())
}

fn show(game: Match) {}

// Tour 6: the API's problem stays typed, and the error converts into a caller's error.
// This compile-only application example prints a missing result or its typed problem.
#[allow(clippy::print_stdout, clippy::print_stderr)]
async fn section_6(riot: &Riot, id: MatchId) -> Result<(), AnyError> {
    match riot.matches(Europe).get(&id).await {
        Ok(game) => show(game),
        Err(e) if e.is_status(404) => println!("no such match"),
        Err(e) => {
            if let Some(problem) = e.problem() {
                eprintln!("Riot says: {}", problem.status.message);
            }
            return Err(e.into());
        }
    }
    Ok(())
}

// Tour 9: both call blocks in the example.
async fn section_9(key: Secret, files: &Files, id: FileId) -> Result<(), AnyError> {
    let riot = Riot::new(key).with_retry(Retry::standard());

    let file = files
        .download(&id)
        .with_timeout(Duration::from_secs(600))
        .await?;
    Ok(())
}

// Tour 10: the runtime now exports fake and reply at the documented root paths.
#[tokio::test]
async fn section_10() -> riot::Result<()> {
    let riot = Riot::new("test-key").with_transport(concord::fake(|request| {
        assert_eq!(request.url().path(), "/lol/match/v5/matches/EUW1_1");
        Ok(concord::reply(200, r#"{ "id": "EUW1_1" }"#))
    }));

    let game = riot.matches(Europe).get(MatchId::from("EUW1_1")).await?;
    Ok(())
}

// Tour 7 contains endpoint descriptions, not call examples. Exercise all five.
async fn section_7(
    files: &Files,
    doc: DocId,
    id: FileId,
    body: concord::Body,
) -> files::Result<()> {
    let exists: bool = files.exists(&doc).await?;
    let readme: String = files.readme("concord").await?;
    let mut download: concord::ByteStream = files.download(&id).await?;
    while let Some(chunk) = download.try_next().await? {
        let size = chunk.len();
    }
    files.upload("example.bin", &body).await?;
    let result: Page = files.search("needle").cursor("next").cursor(None).await?;
    Ok(())
}

// Tour 11 spells the escape hatches in a table. Check those calls too.
// This compile-only application example parses a fixed, valid header fixture.
#[allow(clippy::unwrap_used)]
async fn section_11(riot: &Riot, id: MatchId, http: reqwest::Client) -> riot::Result<()> {
    let call = riot.matches(Europe).get(&id).edit_request(|request| {
        request
            .headers_mut()
            .insert("x-extra", "yes".parse().unwrap());
    });
    let raw: reqwest::Response = call.clone().send_raw().await?;
    let request: reqwest::Request = call.into_request()?;
    let custom = riot.clone().with_transport(http);
    Ok(())
}

// The function pager is compiled where the description is written and uses the
// application's Page::next_cursor through Answer's Deref implementation.
async fn function_pager(feed: &Feed) -> feed::Result<()> {
    let mut pages = feed.list().starting_after("first").pages();
    while let Some(page) = pages.try_next().await? {
        let cursor = page.next_cursor();
    }
    let mut items = feed.list().pages().items();
    while let Some(item) = items.try_next().await? {
        let name = item.name;
    }
    Ok(())
}

// These are the only public names of the generated types; no flat aliases exist.
fn tree_names(
    ids: riot::matches::Ids,
    matches: riot::Matches,
    account: riot::Account,
    error: riot::Error,
) {
    let _: riot::matches::Ids = ids;
    let _: riot::Matches = matches;
    let _: riot::Account = account;
    let _: riot::Error = error;
    let _: tasks::Get = Tasks::new().get(1);
    let _: files::Exists = Files::new("test-key").exists(DocId::from("readme"));
    let _: feed::List = Feed::new().list();
}

fn owned_send<T: Send + 'static>(value: T) {}
fn cloneable<T: Clone>(value: &T) {}

// The references deliberately check that handles and setters accept borrowed inputs.
#[allow(clippy::needless_borrows_for_generic_args)]
#[test]
fn c10_2_ownership_and_arguments() {
    fn shared<T: Clone + Send + Sync + 'static>(_: T) {}
    let riot = Riot::new("key");
    shared(riot.clone());
    shared(riot.matches(Europe));
    shared(riot.matches(Europe).ids(Puuid::from("p")));
    let call = {
        let key = Secret::from("key");
        let puuid = Puuid::from("p-1");
        let riot = Riot::new(&key);
        riot.matches(&Europe)
            .ids(&puuid)
            .start(2)
            .count(5)
            .count(None)
    };
    cloneable(&call);
    owned_send(call.clone());
    owned_send(call.clone().send());
    owned_send(call.pages());

    let call = Tasks::new().list().done(&false).done(Some(true)).done(None);
    owned_send(call.send());
    let token = Secret::from("user-token");
    let me: riot::account::Me = Riot::new("key").account(Europe).me(&token);
    owned_send(me.send_full());
    let name = String::from("Faker");
    let account = Riot::new("key").account(Europe).by_riot_id(&name, "KR1");
    owned_send(account.send());
}

// Every client has the same five settings, including text and parsed bases.
fn client_settings(http: reqwest::Client, base: concord::Base) {
    let tasks = Tasks::new()
        .with_retry(Retry::none())
        .with_timeout(Duration::from_secs(10))
        .with_idle_timeout(Duration::from_secs(1))
        .with_base("http://localhost:8080")
        .with_base(&base)
        .with_transport(http.clone())
        .with_transport(concord::fake(|_| Ok(concord::reply(204, ""))));
    let riot = Riot::new("key")
        .with_retry(Retry::none())
        .with_timeout(Duration::from_secs(10))
        .with_idle_timeout(Duration::from_secs(1))
        .with_base(base.clone())
        .with_transport(http.clone())
        .with_transport(concord::fake(|_| Ok(concord::reply(204, ""))));
    let files = Files::new("test-key")
        .with_retry(Retry::none())
        .with_timeout(Duration::from_secs(10))
        .with_idle_timeout(Duration::from_secs(1))
        .with_base(String::from("http://localhost:8080"))
        .with_transport(http.clone())
        .with_transport(concord::fake(|_| Ok(concord::reply(204, ""))));
    let feed = Feed::new()
        .with_retry(Retry::none())
        .with_timeout(Duration::from_secs(10))
        .with_idle_timeout(Duration::from_secs(1))
        .with_base(base)
        .with_transport(http)
        .with_transport(concord::fake(|_| Ok(concord::reply(204, ""))));
    cloneable(&tasks);
    cloneable(&riot);
    cloneable(&files);
    cloneable(&feed);
}

async fn surface_taskslist(call: tasks::List) -> tasks::Result<()> {
    let call = call
        .with_retry(Retry::none())
        .with_timeout(Duration::from_secs(10))
        .with_idle_timeout(Duration::from_secs(1))
        .edit_request(|request| {
            request.headers_mut().clear();
        });
    let request: reqwest::Request = call.request()?;
    let taken: reqwest::Request = call.clone().into_request()?;
    let raw: reqwest::Response = {
        let sending: concord::Sending<reqwest::Response, ()> = call.clone().send_raw();
        sending.await?
    };
    let full: concord::Answer<Vec<Task>> = {
        let sending: concord::Sending<concord::Answer<Vec<Task>>, ()> = call.clone().send_full();
        sending.await?
    };
    let sent: Vec<Task> = {
        let sending: concord::Sending<Vec<Task>, ()> = call.clone().send();
        sending.await?
    };
    let future: concord::Sending<Vec<Task>, ()> = call.clone().into_future();
    owned_send(future);
    let awaited: Vec<Task> = call.await?;
    Ok(())
}

async fn surface_taskscreate(call: tasks::Create) -> tasks::Result<()> {
    let call = call
        .with_retry(Retry::none())
        .with_timeout(Duration::from_secs(10))
        .with_idle_timeout(Duration::from_secs(1))
        .edit_request(|request| {
            request.headers_mut().clear();
        });
    let request: reqwest::Request = call.request()?;
    let taken: reqwest::Request = call.clone().into_request()?;
    let raw: reqwest::Response = {
        let sending: concord::Sending<reqwest::Response, ()> = call.clone().send_raw();
        sending.await?
    };
    let full: concord::Answer<Task> = {
        let sending: concord::Sending<concord::Answer<Task>, ()> = call.clone().send_full();
        sending.await?
    };
    let sent: Task = {
        let sending: concord::Sending<Task, ()> = call.clone().send();
        sending.await?
    };
    let future: concord::Sending<Task, ()> = call.clone().into_future();
    owned_send(future);
    let awaited: Task = call.await?;
    Ok(())
}

async fn surface_tasksget(call: tasks::Get) -> tasks::Result<()> {
    let call = call
        .with_retry(Retry::none())
        .with_timeout(Duration::from_secs(10))
        .with_idle_timeout(Duration::from_secs(1))
        .edit_request(|request| {
            request.headers_mut().clear();
        });
    let request: reqwest::Request = call.request()?;
    let taken: reqwest::Request = call.clone().into_request()?;
    let raw: reqwest::Response = {
        let sending: concord::Sending<reqwest::Response, ()> = call.clone().send_raw();
        sending.await?
    };
    let full: concord::Answer<Task> = {
        let sending: concord::Sending<concord::Answer<Task>, ()> = call.clone().send_full();
        sending.await?
    };
    let sent: Task = {
        let sending: concord::Sending<Task, ()> = call.clone().send();
        sending.await?
    };
    let future: concord::Sending<Task, ()> = call.clone().into_future();
    owned_send(future);
    let awaited: Task = call.await?;
    Ok(())
}

async fn surface_tasksdelete(call: tasks::Delete) -> tasks::Result<()> {
    let call = call
        .with_retry(Retry::none())
        .with_timeout(Duration::from_secs(10))
        .with_idle_timeout(Duration::from_secs(1))
        .edit_request(|request| {
            request.headers_mut().clear();
        });
    let request: reqwest::Request = call.request()?;
    let taken: reqwest::Request = call.clone().into_request()?;
    let raw: reqwest::Response = {
        let sending: concord::Sending<reqwest::Response, ()> = call.clone().send_raw();
        sending.await?
    };
    let full: concord::Answer<()> = {
        let sending: concord::Sending<concord::Answer<()>, ()> = call.clone().send_full();
        sending.await?
    };
    let sent: () = {
        let sending: concord::Sending<(), ()> = call.clone().send();
        sending.await?
    };
    let future: concord::Sending<(), ()> = call.clone().into_future();
    owned_send(future);
    let awaited: () = call.await?;
    Ok(())
}

async fn surface_riotmatchesids(call: riot::matches::Ids) -> riot::Result<()> {
    let call = call
        .with_retry(Retry::none())
        .with_timeout(Duration::from_secs(10))
        .with_idle_timeout(Duration::from_secs(1))
        .edit_request(|request| {
            request.headers_mut().clear();
        });
    let request: reqwest::Request = call.request()?;
    let taken: reqwest::Request = call.clone().into_request()?;
    let raw: reqwest::Response = {
        let sending: concord::Sending<reqwest::Response, RiotProblem> = call.clone().send_raw();
        sending.await?
    };
    let full: concord::Answer<Vec<MatchId>> = {
        let sending: concord::Sending<concord::Answer<Vec<MatchId>>, RiotProblem> =
            call.clone().send_full();
        sending.await?
    };
    let sent: Vec<MatchId> = {
        let sending: concord::Sending<Vec<MatchId>, RiotProblem> = call.clone().send();
        sending.await?
    };
    let future: concord::Sending<Vec<MatchId>, RiotProblem> = call.clone().send();
    owned_send(future);
    let awaited: Vec<MatchId> = call.send().await?;
    Ok(())
}

async fn surface_riotmatchesget(call: riot::matches::Get) -> riot::Result<()> {
    let call = call
        .with_retry(Retry::none())
        .with_timeout(Duration::from_secs(10))
        .with_idle_timeout(Duration::from_secs(1))
        .edit_request(|request| {
            request.headers_mut().clear();
        });
    let request: reqwest::Request = call.request()?;
    let taken: reqwest::Request = call.clone().into_request()?;
    let raw: reqwest::Response = {
        let sending: concord::Sending<reqwest::Response, RiotProblem> = call.clone().send_raw();
        sending.await?
    };
    let full: concord::Answer<Match> = {
        let sending: concord::Sending<concord::Answer<Match>, RiotProblem> =
            call.clone().send_full();
        sending.await?
    };
    let sent: Match = {
        let sending: concord::Sending<Match, RiotProblem> = call.clone().send();
        sending.await?
    };
    let future: concord::Sending<Match, RiotProblem> = call.clone().into_future();
    owned_send(future);
    let awaited: Match = call.await?;
    Ok(())
}

async fn surface_riotaccountbyriotid(call: riot::account::ByRiotId) -> riot::Result<()> {
    let call = call
        .with_retry(Retry::none())
        .with_timeout(Duration::from_secs(10))
        .with_idle_timeout(Duration::from_secs(1))
        .edit_request(|request| {
            request.headers_mut().clear();
        });
    let request: reqwest::Request = call.request()?;
    let taken: reqwest::Request = call.clone().into_request()?;
    let raw: reqwest::Response = {
        let sending: concord::Sending<reqwest::Response, RiotProblem> = call.clone().send_raw();
        sending.await?
    };
    let full: concord::Answer<Option<Account>> = {
        let sending: concord::Sending<concord::Answer<Option<Account>>, RiotProblem> =
            call.clone().send_full();
        sending.await?
    };
    let sent: Option<Account> = {
        let sending: concord::Sending<Option<Account>, RiotProblem> = call.clone().send();
        sending.await?
    };
    let future: concord::Sending<Option<Account>, RiotProblem> = call.clone().into_future();
    owned_send(future);
    let awaited: Option<Account> = call.await?;
    Ok(())
}

async fn surface_riotaccountme(call: riot::account::Me) -> riot::Result<()> {
    let call = call
        .with_retry(Retry::none())
        .with_timeout(Duration::from_secs(10))
        .with_idle_timeout(Duration::from_secs(1))
        .edit_request(|request| {
            request.headers_mut().clear();
        });
    let request: reqwest::Request = call.request()?;
    let taken: reqwest::Request = call.clone().into_request()?;
    let raw: reqwest::Response = {
        let sending: concord::Sending<reqwest::Response, RiotProblem> = call.clone().send_raw();
        sending.await?
    };
    let full: concord::Answer<Account> = {
        let sending: concord::Sending<concord::Answer<Account>, RiotProblem> =
            call.clone().send_full();
        sending.await?
    };
    let sent: Account = {
        let sending: concord::Sending<Account, RiotProblem> = call.clone().send();
        sending.await?
    };
    let future: concord::Sending<Account, RiotProblem> = call.clone().into_future();
    owned_send(future);
    let awaited: Account = call.await?;
    Ok(())
}

async fn surface_filesexists(call: files::Exists) -> files::Result<()> {
    let call = call
        .with_retry(Retry::none())
        .with_timeout(Duration::from_secs(10))
        .with_idle_timeout(Duration::from_secs(1))
        .edit_request(|request| {
            request.headers_mut().clear();
        });
    let request: reqwest::Request = call.request()?;
    let taken: reqwest::Request = call.clone().into_request()?;
    let raw: reqwest::Response = {
        let sending: concord::Sending<reqwest::Response, ()> = call.clone().send_raw();
        sending.await?
    };
    let full: concord::Answer<bool> = {
        let sending: concord::Sending<concord::Answer<bool>, ()> = call.clone().send_full();
        sending.await?
    };
    let sent: bool = {
        let sending: concord::Sending<bool, ()> = call.clone().send();
        sending.await?
    };
    let future: concord::Sending<bool, ()> = call.clone().into_future();
    owned_send(future);
    let awaited: bool = call.await?;
    Ok(())
}

async fn surface_filesreadme(call: files::Readme) -> files::Result<()> {
    let call = call
        .with_retry(Retry::none())
        .with_timeout(Duration::from_secs(10))
        .with_idle_timeout(Duration::from_secs(1))
        .edit_request(|request| {
            request.headers_mut().clear();
        });
    let request: reqwest::Request = call.request()?;
    let taken: reqwest::Request = call.clone().into_request()?;
    let raw: reqwest::Response = {
        let sending: concord::Sending<reqwest::Response, ()> = call.clone().send_raw();
        sending.await?
    };
    let full: concord::Answer<String> = {
        let sending: concord::Sending<concord::Answer<String>, ()> = call.clone().send_full();
        sending.await?
    };
    let sent: String = {
        let sending: concord::Sending<String, ()> = call.clone().send();
        sending.await?
    };
    let future: concord::Sending<String, ()> = call.clone().into_future();
    owned_send(future);
    let awaited: String = call.await?;
    Ok(())
}

async fn surface_filesdownload(call: files::Download) -> files::Result<()> {
    let call = call
        .with_retry(Retry::none())
        .with_timeout(Duration::from_secs(10))
        .with_idle_timeout(Duration::from_secs(1))
        .edit_request(|request| {
            request.headers_mut().clear();
        });
    let request: reqwest::Request = call.request()?;
    let taken: reqwest::Request = call.clone().into_request()?;
    let raw: reqwest::Response = {
        let sending: concord::Sending<reqwest::Response, ()> = call.clone().send_raw();
        sending.await?
    };
    let full: concord::Answer<concord::ByteStream> = {
        let sending: concord::Sending<concord::Answer<concord::ByteStream>, ()> =
            call.clone().send_full();
        sending.await?
    };
    let sent: concord::ByteStream = {
        let sending: concord::Sending<concord::ByteStream, ()> = call.clone().send();
        sending.await?
    };
    let future: concord::Sending<concord::ByteStream, ()> = call.clone().into_future();
    owned_send(future);
    let awaited: concord::ByteStream = call.await?;
    Ok(())
}

async fn surface_filesupload(call: files::Upload) -> files::Result<()> {
    let call = call
        .with_retry(Retry::none())
        .with_timeout(Duration::from_secs(10))
        .with_idle_timeout(Duration::from_secs(1))
        .edit_request(|request| {
            request.headers_mut().clear();
        });
    let request: reqwest::Request = call.request()?;
    let taken: reqwest::Request = call.clone().into_request()?;
    let raw: reqwest::Response = {
        let sending: concord::Sending<reqwest::Response, ()> = call.clone().send_raw();
        sending.await?
    };
    let full: concord::Answer<()> = {
        let sending: concord::Sending<concord::Answer<()>, ()> = call.clone().send_full();
        sending.await?
    };
    let sent: () = {
        let sending: concord::Sending<(), ()> = call.clone().send();
        sending.await?
    };
    let future: concord::Sending<(), ()> = call.clone().into_future();
    owned_send(future);
    let awaited: () = call.await?;
    Ok(())
}

async fn surface_filessearch(call: files::Search) -> files::Result<()> {
    let call = call
        .with_retry(Retry::none())
        .with_timeout(Duration::from_secs(10))
        .with_idle_timeout(Duration::from_secs(1))
        .edit_request(|request| {
            request.headers_mut().clear();
        });
    let request: reqwest::Request = call.request()?;
    let taken: reqwest::Request = call.clone().into_request()?;
    let raw: reqwest::Response = {
        let sending: concord::Sending<reqwest::Response, ()> = call.clone().send_raw();
        sending.await?
    };
    let full: concord::Answer<Page> = {
        let sending: concord::Sending<concord::Answer<Page>, ()> = call.clone().send_full();
        sending.await?
    };
    let sent: Page = {
        let sending: concord::Sending<Page, ()> = call.clone().send();
        sending.await?
    };
    let future: concord::Sending<Page, ()> = call.clone().into_future();
    owned_send(future);
    let awaited: Page = call.await?;
    Ok(())
}

async fn surface_feedlist(call: feed::List) -> feed::Result<()> {
    let call = call
        .with_retry(Retry::none())
        .with_timeout(Duration::from_secs(10))
        .with_idle_timeout(Duration::from_secs(1))
        .edit_request(|request| {
            request.headers_mut().clear();
        });
    let request: reqwest::Request = call.request()?;
    let taken: reqwest::Request = call.clone().into_request()?;
    let raw: reqwest::Response = {
        let sending: concord::Sending<reqwest::Response, ()> = call.clone().send_raw();
        sending.await?
    };
    let full: concord::Answer<Page> = {
        let sending: concord::Sending<concord::Answer<Page>, ()> = call.clone().send_full();
        sending.await?
    };
    let sent: Page = {
        let sending: concord::Sending<Page, ()> = call.clone().send();
        sending.await?
    };
    let future: concord::Sending<Page, ()> = call.clone().send();
    owned_send(future);
    let awaited: Page = call.send().await?;
    Ok(())
}

// A description uses ordinary Rust imports, in both function and macro namespaces.
mod authored_expressions {
    use concord::{json, Binary, Json, Text};
    use serde_json::json;

    // The author's own function can be used in a body statement too.
    fn own_json(task: &tour::NewTask) -> Result<concord::Body, std::io::Error> {
        Ok(concord::Body::new(task.title.clone(), "text/plain"))
    }

    fn check(task: &tour::NewTask) {
        let _: Result<concord::Body, concord::body::WriteError> = json(task);
        let _: Result<concord::Body, concord::body::WriteError> = json(json!({ "a": 1 }));
        fn reader<R: concord::Reader<T>, T>(_: R) {}
        reader::<_, tour::Task>(Json);
        reader::<_, String>(Text);
        reader::<_, concord::bytes::Bytes>(Binary);
        fn body(_: impl concord::body::IntoBody) {}
        body(own_json(task));
        body(json(task));
        let body_value = concord::Body::new("hello", "text/plain");
        body(&body_value);
        body(body_value);
    }
}

// Owned values, borrowed values, and bare None remain accepted by setters.
// The reference deliberately checks the borrowed numeric setter input.
#[allow(clippy::needless_borrows_for_generic_args)]
fn expanded_arguments() {
    let riot = Riot::new(String::from("key"));
    let count = 5_u32;
    let ids = riot
        .matches(Europe)
        .ids(Puuid::from("p-1"))
        .count(&count)
        .count(None);
    let cursor = String::from("cursor");
    let call = Files::new("test-key")
        .search("needle")
        .cursor("next")
        .cursor(&cursor)
        .cursor(None);
    owned_send(ids);
    owned_send(call);
}

fn response_types(
    answer: concord::Answer<()>,
    incoming: concord::read::Incoming,
    tried: concord::retry::Tried<'_, RiotProblem>,
    error: riot::Error,
    plain_error: tasks::Error,
) {
    let _: http::StatusCode = answer.status();
    let _: http::StatusCode = incoming.status();
    let _: Option<http::StatusCode> = tried.status();
    let _: Option<http::StatusCode> = error.status();
    let _: concord::Error<()> = plain_error;
    let _: bool = error.is_status(404);
    let _: Option<concord::error::Kept> = error.kept();
    let _: Option<&[u8]> = error.body();
    let _: Option<Duration> = error.retry_after();
    let _: Result<RiotProblem, riot::Error> = error.into_problem();
    let _: (http::StatusCode, concord::Headers, ()) = answer.into_parts();
}

fn stream_types(riot: &Riot, feed: &Feed, bytes: concord::ByteStream) {
    fn stream<S: futures_core::Stream<Item = I>, I>(value: S) {}
    stream::<_, Result<Vec<MatchId>, riot::Error>>(
        riot.matches(Europe).ids(Puuid::from("p-1")).pages(),
    );
    stream::<_, Result<MatchId, riot::Error>>(
        riot.matches(Europe).ids(Puuid::from("p-1")).pages().items(),
    );
    stream::<_, Result<Page, feed::Error>>(feed.list().pages());
    stream::<_, Result<FileEntry, feed::Error>>(feed.list().pages().items());
    // D273: "`ByteStream::try_next()` gives a `concord::read::StreamError`".
    stream::<_, Result<bytes::Bytes, concord::read::StreamError>>(bytes);
}

struct LoggedTransport {
    http: reqwest::Client,
}

impl concord::Transport for LoggedTransport {
    async fn send(
        &self,
        request: reqwest::Request,
        info: concord::transport::TryInfo,
    ) -> Result<reqwest::Response, concord::transport::TransportError> {
        let endpoint = info.endpoint();
        let number = info.number();
        concord::Transport::send(&self.http, request, info).await
    }
}

fn transport_future(transport: &LoggedTransport, request: reqwest::Request) {
    fn sendable<F: std::future::Future + Send>(future: F) {}
    let info = concord::transport::TryInfo::new("tasks.get", 1);
    sendable(concord::Transport::send(transport, request, info));
}

// The type of a pager no longer exposes its generated call type.
fn explicit_pager_types(riot: &Riot, feed: &Feed) {
    let _: concord::Pages<Vec<MatchId>, RiotProblem> =
        riot.matches(Europe).ids(Puuid::from("p-1")).pages();
    let _: concord::Items<MatchId, RiotProblem> =
        riot.matches(Europe).ids(Puuid::from("p-1")).pages().items();
    let _: concord::Pages<Page, ()> = feed.list().pages();
    let _: concord::Items<FileEntry, ()> = feed.list().pages().items();
}

// A Basic credential is a value, suitable for an ordinary header entry.
fn credential_values(key: &concord::Secret, borrowed: &str, owned: String) {
    let _: concord::Secret = concord::basic("user", key);
    let _: concord::Secret = concord::basic(borrowed, owned);
}

struct EmptyChunks;

impl futures_core::Stream for EmptyChunks {
    type Item = Result<bytes::Bytes, std::io::Error>;
    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        _: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        std::task::Poll::Ready(None)
    }
}

fn renamed_values() {
    let _: concord::Body = concord::Body::stream(EmptyChunks);
    let _: concord::read::ReadError = concord::read::ReadError::wrong_content_type();
}

#[test]
fn c10_7_debug_only_shows_type_and_endpoint_names() {
    let tasks = Tasks::new();
    assert_eq!(format!("{tasks:?}"), "Tasks { .. }");
    let call = tasks.create(NewTask {
        title: "must not appear".into(),
    });
    assert_eq!(
        format!("{call:?}"),
        "Create { endpoint: \"tasks.create\", .. }"
    );
}

// The two additional descriptions exercise ordinary pager factories as calls.
async fn ordinary_pager_functions(feed: &Feed) -> feed::Result<()> {
    let mut pages: concord::Pages<Page, ()> = feed.by_cursor().starting_after("next").pages();
    while let Some(page) = pages.try_next().await? {
        for item in page {
            let _ = item;
        }
    }
    let mut items: concord::Items<FileEntry, ()> = feed.linked().pages().items();
    while let Some(item) = items.try_next().await? {
        let _ = item;
    }
    let call = concord::page::Call::at_url(feed.linked(), "/linked-items?page=2");
    let _: feed::Linked = call.clone();
    let _: Page = call.send().await?;
    Ok(())
}

// The generated constructor installs its middleware; replacing the sender keeps it.
fn signed_files_constructor(key: concord::Secret) {
    let _: Files = Files::new(key).with_transport(concord::fake(|_| {
        Ok(concord::reqwest::Response::from(
            concord::http::Response::new(""),
        ))
    }));
}

#[tokio::test]
async fn signature_covers_the_actual_method_and_path() {
    use concord::transport::{Transport, TryInfo};
    use concord::{http, reqwest};
    use concord::{Middleware, Next};

    let signed = Signed::new("test-key");
    let transport = concord::fake(|request| {
        let mut response = http::Response::new("");
        response
            .headers_mut()
            .insert("x-signature", request.headers()["x-signature"].clone());
        Ok(reqwest::Response::from(response))
    });
    async fn signature(
        signed: &impl Middleware,
        transport: &impl Transport,
        method: http::Method,
        url: &str,
    ) -> http::HeaderValue {
        let request = reqwest::Request::new(method, url.parse().unwrap());
        let response = signed
            .handle(
                request,
                TryInfo::new("files.exists", 1),
                Next::to(transport),
            )
            .await
            .unwrap();
        response.headers()["x-signature"].clone()
    }
    let original = signature(
        &signed,
        &transport,
        http::Method::GET,
        "https://files.example.com/a",
    )
    .await;
    assert!(original.is_sensitive());
    assert_eq!(
        original,
        signature(
            &signed,
            &transport,
            http::Method::GET,
            "https://files.example.com/a?x=1"
        )
        .await
    );
    assert_ne!(
        original,
        signature(
            &signed,
            &transport,
            http::Method::POST,
            "https://files.example.com/a"
        )
        .await
    );
    assert_ne!(
        original,
        signature(
            &signed,
            &transport,
            http::Method::GET,
            "https://files.example.com/b"
        )
        .await
    );
}

#[tokio::test]
async fn the_transport_below_receives_the_signed_request() {
    use concord::transport::TryInfo;
    use concord::{http, reqwest};
    use concord::{Middleware, Next};
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    let called = Arc::new(AtomicBool::new(false));
    let seen = called.clone();
    let signed = Signed::new("test-key");
    let transport = concord::fake(move |request| {
        let signature = &request.headers()["x-signature"];
        assert!(signature.is_sensitive());
        assert_eq!(signature.to_str().unwrap(), "2e65ac4a944b7759");
        seen.store(true, Ordering::SeqCst);
        Ok(reqwest::Response::from(http::Response::new("")))
    });
    let request = reqwest::Request::new(
        http::Method::HEAD,
        "https://files.example.com/a".parse().unwrap(),
    );
    let response = signed
        .handle(
            request,
            TryInfo::new("files.exists", 1),
            Next::to(&transport),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), http::StatusCode::OK);
    assert!(called.load(Ordering::SeqCst));
}

#[tokio::test]
async fn two_middlewares_reach_the_fake_in_order_and_unwind_in_reverse() {
    use concord::transport::{TransportError, TryInfo};
    use concord::{http, reqwest, Middleware, Next};
    use std::sync::{Arc, Mutex};

    struct Record {
        name: &'static str,
        events: Arc<Mutex<Vec<String>>>,
    }
    impl Middleware for Record {
        async fn handle(
            &self,
            mut request: reqwest::Request,
            info: TryInfo,
            next: Next<'_>,
        ) -> Result<reqwest::Response, TransportError> {
            assert_eq!((info.endpoint(), info.number()), ("files.exists", 2));
            self.events
                .lock()
                .unwrap()
                .push(format!("{} in", self.name));
            request
                .headers_mut()
                .append("x-order", self.name.parse().unwrap());
            let mut response = next.send(request, info).await?;
            self.events
                .lock()
                .unwrap()
                .push(format!("{} out", self.name));
            response
                .headers_mut()
                .append("x-order", self.name.parse().unwrap());
            Ok(response)
        }
    }

    let events = Arc::new(Mutex::new(Vec::new()));
    let sent = events.clone();
    let transport = concord::fake(move |request| {
        let order: Vec<_> = request
            .headers()
            .get_all("x-order")
            .iter()
            .map(|v| v.to_str().unwrap())
            .collect();
        assert_eq!(order, ["caller", "description"]);
        sent.lock().unwrap().push("transport".into());
        Ok(reqwest::Response::from(http::Response::new("")))
    });
    let core = concord::__private::Core::<()>::default()
        .add_middleware(Record {
            name: "description",
            events: events.clone(),
        })
        .with_middleware(Record {
            name: "caller",
            events: events.clone(),
        });
    let request = reqwest::Request::new(
        http::Method::HEAD,
        "https://files.example.com/a".parse().unwrap(),
    );
    let response = core
        .next(&transport)
        .send(request, TryInfo::new("files.exists", 2))
        .await
        .unwrap();
    let order: Vec<_> = response
        .headers()
        .get_all("x-order")
        .iter()
        .map(|v| v.to_str().unwrap())
        .collect();
    assert_eq!(order, ["description", "caller"]);
    assert_eq!(
        *events.lock().unwrap(),
        [
            "caller in",
            "description in",
            "transport",
            "description out",
            "caller out",
        ]
    );
}

#[tokio::test]
async fn a_middleware_can_send_twice_through_the_same_remainder() {
    use concord::transport::{TransportError, TryInfo};
    use concord::{http, reqwest, Middleware, Next};
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    struct Renew;
    impl Middleware for Renew {
        async fn handle(
            &self,
            mut request: reqwest::Request,
            info: TryInfo,
            next: Next<'_>,
        ) -> Result<reqwest::Response, TransportError> {
            request
                .headers_mut()
                .insert("authorization", "old".parse().unwrap());
            let mut again = request.try_clone().unwrap();
            let response = next.send(request, info.clone()).await?;
            assert_eq!(response.status(), http::StatusCode::UNAUTHORIZED);
            again
                .headers_mut()
                .insert("authorization", "renewed".parse().unwrap());
            next.send(again, info).await
        }
    }
    struct Count(AtomicUsize);
    impl Middleware for Count {
        async fn handle(
            &self,
            mut request: reqwest::Request,
            info: TryInfo,
            next: Next<'_>,
        ) -> Result<reqwest::Response, TransportError> {
            assert_eq!((info.endpoint(), info.number()), ("files.search", 1));
            let count = self.0.fetch_add(1, Ordering::SeqCst) + 1;
            request
                .headers_mut()
                .insert("x-count", count.to_string().parse().unwrap());
            next.send(request, info).await
        }
    }

    let sends = Arc::new(AtomicUsize::new(0));
    let seen = sends.clone();
    let transport = concord::fake(move |request| {
        let count = seen.fetch_add(1, Ordering::SeqCst) + 1;
        assert_eq!(request.headers()["x-count"], count.to_string());
        assert_eq!(request.body().unwrap().as_bytes().unwrap(), b"payload");
        assert_eq!(
            request.headers()["authorization"],
            if count == 1 { "old" } else { "renewed" }
        );
        let mut response = http::Response::new("");
        *response.status_mut() = if count == 1 {
            http::StatusCode::UNAUTHORIZED
        } else {
            http::StatusCode::OK
        };
        Ok(reqwest::Response::from(response))
    });
    let core = concord::__private::Core::<()>::default()
        .add_middleware(Renew)
        .add_middleware(Count(AtomicUsize::new(0)));
    let mut request = reqwest::Request::new(
        http::Method::POST,
        "https://files.example.com/search".parse().unwrap(),
    );
    *request.body_mut() = Some("payload".into());
    let response = core
        .next(&transport)
        .send(request, TryInfo::new("files.search", 1))
        .await
        .unwrap();
    assert_eq!(response.status(), http::StatusCode::OK);
    assert_eq!(sends.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn a_middleware_can_block_without_sending() {
    use concord::transport::{TransportError, TryInfo};
    use concord::{http, reqwest, Middleware, Next};

    struct Block;
    impl Middleware for Block {
        async fn handle(
            &self,
            _: reqwest::Request,
            _: TryInfo,
            _: Next<'_>,
        ) -> Result<reqwest::Response, TransportError> {
            Err(TransportError::blocked())
        }
    }
    let transport = concord::fake(|_| panic!("a blocked request must not reach the transport"));
    let core = concord::__private::Core::<()>::default().add_middleware(Block);
    let request = reqwest::Request::new(
        http::Method::HEAD,
        "https://files.example.com/a".parse().unwrap(),
    );
    let error = core
        .next(&transport)
        .send(request, TryInfo::new("files.exists", 1))
        .await
        .unwrap_err();
    assert_eq!(error.class(), concord::error::Class::Blocked);
}

// Reading an independently obtained response uses every endpoint's own value and
// problem types, exercised below against complete response fixtures.
#[tokio::test]
async fn read_independent_responses() -> Result<(), AnyError> {
    fn response(body: &'static str) -> reqwest::Response {
        concord::reply_with(200, &[("content-type", "application/json")], body)
    }
    let tasks = Tasks::new();
    let _: Vec<Task> = tasks.list().read_response(response(r#"[]"#)).await?;
    let _: Task = tasks
        .create(NewTask {
            title: "A task".into(),
        })
        .read_response(response(r#"{"id":1,"title":"A task","done":false}"#))
        .await?;
    let _: Task = tasks
        .get(1)
        .read_response(response(r#"{"id":1,"title":"A task","done":false}"#))
        .await?;
    let _: () = tasks.delete(1).read_response(response("")).await?;
    let riot = Riot::new("key");
    let _: Vec<MatchId> = riot
        .matches(Europe)
        .ids(Puuid::from("p-1"))
        .read_response(response(r#"["EUW1_1"]"#))
        .await?;
    let _: Match = riot
        .matches(Europe)
        .get(MatchId::from("EUW1_1"))
        .read_response(response(r#"{"id":"EUW1_1"}"#))
        .await?;
    let _: Option<Account> = riot
        .account(Europe)
        .by_riot_id("Faker", "KR1")
        .read_response(response(
            r#"{"puuid":"p-1","gameName":"Faker","tagLine":"KR1"}"#,
        ))
        .await?;
    let _: Account = riot
        .account(Europe)
        .me("token")
        .read_response(response(
            r#"{"puuid":"p-1","gameName":"Faker","tagLine":"KR1"}"#,
        ))
        .await?;
    let files = Files::new("test-key");
    let _: bool = files
        .exists(DocId::from("readme"))
        .read_response(response(""))
        .await?;
    let _: String = files
        .readme("concord")
        .read_response(response("read me"))
        .await?;
    let _: concord::ByteStream = files
        .download(FileId::from("file-1"))
        .read_response(response("bytes"))
        .await?;
    let _: () = files
        .upload("file.txt", concord::Body::new("text", "text/plain"))
        .read_response(response(""))
        .await?;
    let _: Page = files
        .search("needle")
        .read_response(response(r#"{"data":[],"next_cursor":null}"#))
        .await?;
    let feed = Feed::new();
    let _: Page = feed
        .list()
        .read_response(response(r#"{"data":[],"next_cursor":null}"#))
        .await?;
    let _: Page = feed
        .by_cursor()
        .read_response(response(r#"{"data":[],"next_cursor":null}"#))
        .await?;
    let _: Page = feed
        .linked()
        .read_response(response(r#"{"data":[],"next_cursor":null}"#))
        .await?;
    Ok(())
}

fn read_response_is_send(call: &tasks::List, response: reqwest::Response) {
    fn send<F: std::future::Future<Output = tasks::Result<Vec<Task>>> + Send>(_: F) {}
    send(call.read_response(response));
}

#[test]
fn customized_http_transport() -> Result<(), reqwest::Error> {
    let http = concord::Http::with(|builder| {
        builder
            .connect_timeout(Duration::from_secs(5))
            .referer(true)
            .redirect(reqwest::redirect::Policy::limited(5))
            .retry(reqwest::retry::for_host("tasks.example.com"))
    })?;
    let _: Tasks = Tasks::new().with_transport(http);
    Ok(())
}

#[test]
fn concord_http_can_be_constructed() {
    let http = concord::Http::new().expect("Concord's default HTTP client should build");
    fn transport<T: concord::Transport>(_: &T) {}
    transport(&http);
}

#[test]
fn optional_string_lists_accept_text_arrays_and_slices() {
    use concord::arg::Arg;
    fn labels(input: impl Arg<Option<Vec<String>>>) -> Option<Vec<String>> {
        input.into_input()
    }
    let expected = Some(vec!["bug".to_owned(), "ui".to_owned()]);
    assert_eq!(labels(["bug", "ui"]), expected);
    assert_eq!(labels(["bug", "ui"].as_slice()), expected);
    assert_eq!(labels([] as [&str; 0]), Some(Vec::new()));
    assert_eq!(labels(None), None);
}

fn borrowed_reader_and_secret(reader: &impl concord::Reader<Task>, key: &Secret) {
    fn reader_bound(_: impl concord::Reader<Task>) {}
    reader_bound(concord::__private::reader::<Task, _>(reader));
    fn value_bound(_: impl concord::__private::Value) {}
    value_bound(key);
}

// Generated placements adapt every value, including optional secrets, before the
// draft sees it. These compile-only checks deliberately never build a request.
fn optional_secret_header_and_template(
    draft: &mut concord::__private::Draft,
    token: Option<Secret>,
) {
    use concord::__private::{AdaptValue as _, Piece, ValueRef};
    draft.header(
        "x-token",
        &[Piece::Val("x-token", &(&ValueRef(&token)).adapt())],
    );
    draft.header(
        "authorization",
        &[
            Piece::Lit("Bearer "),
            Piece::Val("token", &(&ValueRef(&token)).adapt()),
        ],
    );
}

fn owner_views_lend_inputs_to_expressions(key: &Secret, scope_token: &Option<Secret>) {
    struct Client<'a> {
        key: &'a Secret,
    }
    struct Scope<'a> {
        token: &'a Option<Secret>,
    }
    let client = Client { key };
    let scope = Scope { token: scope_token };
    let _: &Secret = client.key;
    let _: &Option<Secret> = scope.token;
    let _: Secret = concord::basic(client.key, "");
}

// An expression containing `?` runs inside a boxed-error closure. The hidden
// helper preserves that error as the Build error's source, in the engine.
fn fallible_entry_expression(
    draft: &mut concord::__private::Draft,
    limit: &str,
) -> Result<(), concord::Error> {
    use concord::__private::{AdaptValue as _, Piece, ValueRef};
    let value = (|| -> Result<_, AnyError> { Ok(limit.parse::<u32>()?) })().map_err(|source| {
        concord::__private::Draft::entry_error("example.list", "limit", source)
    })?;
    draft.query("limit", &[Piece::Val("limit", &(&ValueRef(value)).adapt())]);
    Ok(())
}

// A Result arrow reads unclaimed successes as T and wraps them in Ok.
// A HEAD with an arrow still needs an explicit success arm.
async fn result_arrow_without_a_matching_arm(
    response: reqwest::Response,
    info: concord::transport::TryInfo,
) -> Result<Result<u32, String>, concord::Error> {
    match response.status() {
        http::StatusCode::OK => Ok(Ok(7)),
        status if status.is_success() => concord::__private::read_body(
            &concord::__private::reader::<u32, _>(concord::Json),
            response,
            &info,
        )
        .await
        .map(Ok),
        _ => Err(concord::__private::refused_plain(response, &info).await),
    }
}

async fn head_arrow_without_a_matching_arm(
    response: reqwest::Response,
    info: concord::transport::TryInfo,
) -> Result<bool, concord::Error> {
    match response.status() {
        http::StatusCode::OK => Ok(true),
        status if status.is_success() => Err(concord::__private::no_arm(response, &info).await),
        _ => Err(concord::__private::refused_plain(response, &info).await),
    }
}

#[test]
fn the_value_adapter_accepts_owned_and_borrowed_secret_shapes() {
    use concord::__private::{AdaptValue as _, Piece, ValueRef};
    // This test only creates template pieces: it never calls the engine's writer.
    let token = Secret::new("private-token");
    let optional = Some(token.clone());
    let borrowed = &optional;
    let nested_borrow = &borrowed;
    let absent: Option<Secret> = None;
    let nested = Some(Some(token.clone()));
    let nested_null = Some(None::<Secret>);
    let nested_absent = None::<Option<Secret>>;
    for debug in [
        format!(
            "{:?}",
            Piece::Val("token", &(&ValueRef(token.clone())).adapt())
        ),
        format!("{:?}", Piece::Val("token", &(&ValueRef(&token)).adapt())),
        format!(
            "{:?}",
            Piece::Val("token", &(&ValueRef(optional.clone())).adapt())
        ),
        format!("{:?}", Piece::Val("token", &(&ValueRef(borrowed)).adapt())),
        format!(
            "{:?}",
            Piece::Val("token", &(&ValueRef(nested_borrow)).adapt())
        ),
        format!("{:?}", Piece::Val("token", &(&ValueRef(absent)).adapt())),
        format!("{:?}", Piece::Val("token", &(&ValueRef(&nested)).adapt())),
        format!(
            "{:?}",
            Piece::Val("token", &(&ValueRef(&nested_null)).adapt())
        ),
        format!(
            "{:?}",
            Piece::Val("token", &(&ValueRef(&nested_absent)).adapt())
        ),
        format!("{:?}", Piece::Val("limit", &(&ValueRef(5_u32)).adapt())),
        format!(
            "{:?}",
            Piece::Val("ids", &(&ValueRef(vec!["one", "two"])).adapt())
        ),
    ] {
        assert!(!debug.contains("private-token"));
    }
}

// GET and HEAD retain headers on a conditional result, including nested arrows.
fn conditional_send_types(tasks: &Tasks, files: &Files, riot: &Riot, feed: &Feed) {
    let _: concord::Sending<concord::Answer<Option<Task>>> =
        tasks.get(1).send_if_none_match("etag");
    let _: concord::Sending<concord::Answer<Option<bool>>> =
        files.exists(DocId::from("a")).send_if_none_match("etag");
    let _: concord::Sending<concord::Answer<Option<Option<Account>>>, RiotProblem> = riot
        .account(Europe)
        .by_riot_id("a", "b")
        .send_if_none_match("etag");
    let _: concord::Sending<concord::Answer<Option<Page>>> = feed.list().send_if_none_match("etag");
}

#[test]
fn optional_optional_inputs_keep_absent_null_and_value_apart() {
    use concord::arg::Arg;
    fn milestone(value: impl Arg<Option<Option<u64>>>) -> Option<Option<u64>> {
        value.into_input()
    }
    assert_eq!(milestone(5), Some(Some(5)));
    assert_eq!(milestone(Some(5)), Some(Some(5)));
    assert_eq!(milestone(None::<u64>), Some(None));
    assert_eq!(milestone(Some(None)), Some(None));
    assert_eq!(milestone(None::<Option<u64>>), None);
}

#[test]
fn inherited_ordinary_values_and_path_holes_pass_before_erasure() {
    use concord::__private::{inherited, AdaptValue, Piece, Value, ValueRef};
    let number = 7_u32;
    let ordinary = Some("text");
    let _: &dyn Value = &inherited((&ValueRef(&number)).adapt());
    let _ = Piece::Val("path", &inherited((&ValueRef(&ordinary)).adapt()));
}

#[tokio::test]
async fn caller_order_and_description_order_are_each_preserved() {
    use concord::transport::{TransportError, TryInfo};
    use concord::{Middleware, Next};
    struct Append(&'static str);
    impl Middleware for Append {
        async fn handle(
            &self,
            mut request: reqwest::Request,
            info: TryInfo,
            next: Next<'_>,
        ) -> Result<reqwest::Response, TransportError> {
            request
                .headers_mut()
                .append("x-order", self.0.parse().unwrap());
            next.send(request, info).await
        }
    }
    // Even an interleaved setup keeps callers before the description's own order.
    let core = concord::__private::Core::<()>::default()
        .add_middleware(Append("description-1"))
        .with_middleware(Append("caller-1"))
        .add_middleware(Append("description-2"))
        .with_middleware(Append("caller-2"));
    let transport = concord::fake(|request| {
        let order: Vec<_> = request
            .headers()
            .get_all("x-order")
            .iter()
            .map(|value| value.to_str().unwrap())
            .collect();
        assert_eq!(
            order,
            ["caller-1", "caller-2", "description-1", "description-2"]
        );
        Ok(reqwest::Response::from(http::Response::new("")))
    });
    let request =
        reqwest::Request::new(http::Method::GET, "https://example.com/a".parse().unwrap());
    core.next(&transport)
        .send(request, TryInfo::new("tasks.get", 1))
        .await
        .unwrap();
}

// The plain read closure may borrow a call input; it needs no 'static lifetime.
// The invoked closure deliberately checks the reader boundary required by docs/macro.md.
#[allow(clippy::redundant_closure_call)]
fn read_closures_can_borrow_inputs(input: &str) {
    struct Borrowing<'a>(&'a str);
    impl concord::Reader<String> for Borrowing<'_> {
        async fn read(
            &self,
            _: concord::read::Incoming,
        ) -> Result<String, concord::read::ReadError> {
            Ok(self.0.to_owned())
        }
    }
    fn accepts(_: impl concord::Reader<String>) {}
    accepts((|| {
        concord::__private::reader::<String, _>(Borrowing(input))
    })());
}

fn fixture_tasks() -> Tasks {
    Tasks::new().with_transport(concord::fake(|request| {
        let body = if request.url().path() == "/api/tasks" && request.method() == http::Method::GET
        {
            r#"[{"id":1,"title":"A task","done":false}]"#
        } else {
            r#"{"id":1,"title":"A task","done":false}"#
        };
        Ok(concord::reply_with(
            200,
            &[("content-type", "application/json")],
            body,
        ))
    }))
}
fn fixture_riot() -> Riot {
    Riot::new("key").with_transport(concord::fake(|request| {
        let path = request.url().path();
        let body = if path.ends_with("/ids") {
            r#"["EUW1_1"]"#
        } else if path.contains("/matches/") {
            r#"{"id":"EUW1_1"}"#
        } else {
            r#"{"puuid":"p-1","gameName":"Faker","tagLine":"KR1"}"#
        };
        Ok(concord::reply_with(
            200,
            &[("content-type", "application/json")],
            body,
        ))
    }))
}
fn fixture_files() -> Files {
    Files::new("key").with_transport(concord::fake(|request| {
        let body = if request.url().path() == "/search" {
            r#"{"data":[],"next_cursor":null}"#
        } else {
            "bytes"
        };
        Ok(concord::reply_with(
            200,
            &[("content-type", "application/json")],
            body,
        ))
    }))
}

#[tokio::test]
async fn c3_c10_every_non_page_surface_now_runs() -> Result<(), AnyError> {
    let tasks = fixture_tasks();
    surface_taskslist(tasks.list()).await?;
    surface_taskscreate(tasks.create(NewTask {
        title: "A task".into(),
    }))
    .await?;
    surface_tasksget(tasks.get(1)).await?;
    surface_tasksdelete(tasks.delete(1)).await?;
    let riot = fixture_riot();
    surface_riotmatchesids(riot.matches(Europe).ids(Puuid::from("p-1"))).await?;
    surface_riotmatchesget(riot.matches(Europe).get(MatchId::from("EUW1_1"))).await?;
    surface_riotaccountbyriotid(riot.account(Europe).by_riot_id("Faker", "KR1")).await?;
    surface_riotaccountme(riot.account(Europe).me("token")).await?;
    let files = fixture_files();
    surface_filesexists(files.exists(DocId::from("doc"))).await?;
    surface_filesreadme(files.readme("repo")).await?;
    surface_filesdownload(files.download(FileId::from("file"))).await?;
    surface_filesupload(files.upload("f", concord::Body::new("bytes", "text/plain"))).await?;
    surface_filessearch(files.search("needle")).await?;
    let feed = Feed::new().with_transport(concord::fake(|_| {
        Ok(concord::reply(200, r#"{"data":[],"next_cursor":null}"#))
    }));
    surface_feedlist(feed.list()).await?;
    Ok(())
}

#[tokio::test]
async fn c3_c10_tour_examples_without_page_iteration_now_run() -> Result<(), AnyError> {
    section_1(fixture_tasks()).await?;
    let riot = fixture_riot();
    section_3(&riot, Puuid::from("p-1")).await?;
    section_4(&riot).await?;
    section_6(&riot, MatchId::from("EUW1_1")).await?;
    let files = fixture_files();
    section_7(
        &files,
        DocId::from("doc"),
        FileId::from("file"),
        concord::Body::new("bytes", "text/plain"),
    )
    .await?;
    section_9(Secret::new("key"), &files, FileId::from("file")).await?;
    section_11(&riot, MatchId::from("EUW1_1"), reqwest::Client::new()).await?;
    Ok(())
}

#[tokio::test]
async fn c5_result_and_head_without_matching_arms_now_run() {
    let info = concord::transport::TryInfo::new("test.read", 0);
    assert_eq!(
        result_arrow_without_a_matching_arm(concord::reply(201, "7"), info.clone())
            .await
            .unwrap(),
        Ok(7)
    );
    assert!(
        head_arrow_without_a_matching_arm(concord::reply(204, ""), info)
            .await
            .unwrap_err()
            .kind()
            == concord::error::Kind::Mismatch
    );
}

#[tokio::test(start_paused = true)]
async fn c8_7_tour_section_5_runs_the_items_loop() {
    let script = concord_testkit::Script::new();
    script.answer(200).body(r#"["one"]"#);
    script.answer(200).body("[]");
    let riot = Riot::new("key").with_transport(script.clone());
    section_5(&riot, Puuid::from("p")).await.unwrap();
    assert_eq!(script.requests().len(), 2);
    assert_eq!(script.unused(), 0);
}

#[tokio::test(start_paused = true)]
async fn c8_4_tour_function_pager_runs_both_page_and_item_loops() {
    let script = concord_testkit::Script::new();
    script
        .answer(200)
        .body(r#"{"data":[],"next_cursor":"next"}"#);
    script
        .answer(200)
        .body(r#"{"data":[{"id":"f","name":"first"}],"next_cursor":null}"#);
    script
        .answer(200)
        .body(r#"{"data":[{"id":"g","name":"second"}],"next_cursor":null}"#);
    let feed = Feed::new().with_transport(script.clone());
    function_pager(&feed).await.unwrap();
    assert_eq!(script.requests().len(), 3);
    assert_eq!(script.unused(), 0);
}

#[tokio::test(start_paused = true)]
async fn c8_6_tour_ordinary_pagers_run_cursor_link_and_at_url_calls() {
    let script = concord_testkit::Script::new();
    script
        .answer(200)
        .body(r#"{"data":[],"next_cursor":"following"}"#);
    script
        .answer(200)
        .body(r#"{"data":[{"id":"f","name":"cursor"}],"next_cursor":null}"#);
    script
        .answer(200)
        .header("link", r#"</linked-items?page=2>; rel="next""#)
        .body(r#"{"data":[],"next_cursor":null}"#);
    script
        .answer(200)
        .body(r#"{"data":[{"id":"g","name":"linked"}],"next_cursor":null}"#);
    script.answer(200).body(r#"{"data":[],"next_cursor":null}"#);
    let feed = Feed::new().with_transport(script.clone());
    ordinary_pager_functions(&feed).await.unwrap();
    assert_eq!(script.requests().len(), 5);
    assert_eq!(script.unused(), 0);
}

// The model of the guide (ANSWERS.md A38) and the client's `Retry` (D270) ------------

// Chapter 6, section 6.3, as the guide writes it: the client's `Retry` names the problem
// type, so a rule reads `tried.problem()` with no annotation.
#[tokio::test(start_paused = true)]
async fn c4_7_a_rule_of_chapter_6_reads_the_problem_through_the_clients_retry() {
    let rule = riot::Retry::standard().unless(|tried| {
        tried
            .problem()
            .is_some_and(|p| p.status.message == "maintenance")
    });
    let script = concord_testkit::Script::new();
    script
        .answer(503)
        .header("content-type", "application/json")
        .body(r#"{"status":{"message":"maintenance","status_code":503}}"#);
    script.answer(200).body(r#"{"id":"EUW1_1"}"#);
    let riot = Riot::new("key")
        .with_transport(script.clone())
        .with_retry(rule);
    let error = riot
        .matches(Europe)
        .get(MatchId::from("EUW1_1"))
        .await
        .unwrap_err();
    assert!(error.is_status(503));
    assert_eq!(error.tries(), 1, "the rule ended the tries");
    assert_eq!(script.unused(), 1);

    // Every client has one, beside `Error` and `Result`.
    let _: tasks::Retry = tasks::Retry::standard();
    let _: files::Retry = files::Retry::none();
    let _: feed::Retry = feed::Retry::standard().tries(2);
    let _: concord::Retry<ShapesProblem> = shapes::Retry::standard();
}

// Chapter 3, the sketch, and chapter 4, section 4.11: `ids` has `queue` and `start_time`.
#[test]
fn c2_5_riot_ids_writes_the_four_options_of_chapter_4() {
    let request = Riot::new("key")
        .matches(Asia)
        .ids(Puuid::from("p-1"))
        .start(10)
        .count(5)
        .queue(420)
        .start_time(1_700_000_000)
        .request()
        .unwrap();
    assert_eq!(
        request.url().as_str(),
        "https://asia.api.riotgames.com/lol/match/v5/matches/by-puuid/p-1/ids\
         ?start=10&count=5&queue=420&startTime=1700000000"
    );
    let request = Riot::new("key")
        .matches(Asia)
        .ids(Puuid::from("p-1"))
        .queue(420)
        .queue(None)
        .request()
        .unwrap();
    assert_eq!(request.url().query(), None);
}

// Chapter 5, section 5.6: Riot has no client arm. A refusal keeps its bytes, and the
// problem is read from them by the `problem` line.
#[tokio::test(start_paused = true)]
async fn c5_6_a_riot_refusal_keeps_its_bytes_and_its_problem() {
    let body = r#"{"status":{"message":"slow down","status_code":429}}"#;
    let script = concord_testkit::Script::new();
    script
        .answer(429)
        .header("content-type", "application/json")
        .body(body);
    let error = Riot::new("key")
        .with_transport(script.clone())
        .matches(Europe)
        .get(MatchId::from("EUW1_1"))
        .await
        .unwrap_err();
    assert_eq!(error.kind(), concord::error::Kind::Refused);
    assert_eq!(error.problem().unwrap().status.message, "slow down");
    assert_eq!(error.body(), Some(body.as_bytes()));
}

// Chapter 5, section 5.8: `body json { text; cursor; }`; a cursor that was not set is left out.
#[test]
fn c2_14_files_search_is_a_body_of_its_two_inputs() {
    let files = Files::new("key");
    let request = files.search("needle").request().unwrap();
    assert_eq!(
        request.body().and_then(reqwest::Body::as_bytes),
        Some(&br#"{"text":"needle"}"#[..])
    );
    let request = files.search("needle").cursor("next").request().unwrap();
    assert_eq!(
        request.body().and_then(reqwest::Body::as_bytes),
        Some(&br#"{"text":"needle","cursor":"next"}"#[..])
    );
}
