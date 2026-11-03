//! The quick start of `docs/QUICKSTART.md`: one Concord declaration, a client built from the
//! application's `reqwest::Client`, two calls and the handling of an API error document.
//!
//! `cargo run -p concord-example-quickstart` starts [`local_server`] (a stand-in for the real
//! service on 127.0.0.1) and calls [`run`] against it. The tests check what went over the wire,
//! run the code quoted in `docs/CALLER_GUIDE.md` and compare every quoted excerpt in the
//! documentation with its source.
// The README quotes this file: field-by-field docs would only lengthen the quote.
#![allow(missing_docs)]

pub mod local_server;

use concord::reqwest::{self, header::AUTHORIZATION};
use concord::{Decode, Json};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct Task {
    pub id: u64,
    pub title: String,
    pub done: bool,
}

#[derive(Debug, Default, Serialize)]
pub struct NewTask {
    pub title: String,
}

/// The error document the API sends with a non-2xx status.
#[derive(Debug, Deserialize)]
pub struct Problem {
    pub title: String,
}

concord::api! {
    /// A small task-tracking API.
    pub client Tasks(#[sensitive] token: String) {
        base "https://tasks.example.com/api";
        headers { AUTHORIZATION = "Bearer {token}"; }
        format Json;

        scope project(project: String) {
            path "projects/{project}";

            GET tasks(query done?: bool, query limit: u32 = 20) -> Vec<Task> {
                path "tasks";
            }
            POST create(body task: NewTask) -> Task {
                path "tasks";
            }
        }
    }
}

pub async fn run(base_url: &str, token: String) -> Result<(), Box<dyn std::error::Error>> {
    // Transport settings (timeouts, proxies, TLS) belong to your reqwest::Client.
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    // `with_base_url` points the client at the local stand-in; leave it out for the real API.
    let api = Tasks::new(http, TasksConfig { token }).with_base_url(base_url)?;
    println!("{api:?}"); // the token is shown as <redacted>

    let project = api.project("demo");
    let title = "Write the quick start".to_owned();
    let task = project.create(NewTask { title }).send().await?; // POST .../projects/demo/tasks
    println!("created task #{}: {}", task.id, task.title);

    let open = project.tasks().set_done(false).send().await?; // GET ...?done=false&limit=20
    println!("{} open task(s)", open.len());

    // This API rejects an empty title: a non-2xx status is an error that keeps the response.
    match project.create(NewTask::default()).send().await {
        Ok(task) => println!("unexpectedly created task #{}", task.id),
        Err(error) if error.status().is_some() => {
            let response = error.into_response()?; // a status error keeps its response
            let status = response.status();
            let problem = <Json as Decode<Problem>>::decode_body((), response.bytes().await?)?;
            println!("rejected with {status}: {}", problem.title);
        }
        Err(error) => return Err(error.into()), // any other kind of error propagates
    }
    Ok(())
}

/// The code blocks of `docs/AUTHOR_GUIDE.md`, compiled and run as doctests.
#[cfg(doctest)]
#[doc = include_str!("../../../docs/AUTHOR_GUIDE.md")]
pub struct AuthorGuide;

/// The code of `docs/CHEATSHEET.md`, compiled and run as a doctest.
#[cfg(doctest)]
#[doc = include_str!("../../../docs/CHEATSHEET.md")]
pub struct CheatSheet;

/// The current-syntax code blocks of `docs/MIGRATION.md`, compiled and run as doctests.
#[cfg(doctest)]
#[doc = include_str!("../../../docs/MIGRATION.md")]
pub struct Migration;
