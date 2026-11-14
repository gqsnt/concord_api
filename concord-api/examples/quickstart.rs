//! A client and one call, with no network.
//!
//! Run it with `cargo run --example quickstart`.
#![allow(clippy::print_stdout)] // An example prints what it shows.

use concord::{fake, json, reply, Json};
use serde::{Deserialize, Serialize};

// The data of the API are plain `serde` types. Concord does not see inside them.
#[derive(Debug, Deserialize)]
struct Task {
    id: u64,
    title: String,
    done: bool,
}

// An input is `Clone`: a call can be cloned and sent again.
#[derive(Clone, Serialize)]
struct NewTask {
    title: String,
}

// The description. `base` and `read` are written once, on the client, and every endpoint
// below inherits them. The signature is for the caller; the block says where each input goes.
concord::api! {
    client Tasks {
        base "https://tasks.example.com/api/tasks";
        read Json;

        POST create(task: NewTask) -> Task {
            body json(task);
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> tasks::Result<()> {
    // `with_transport` puts a function in place of the network. It gets the request that
    // would go out, and gives the answer. A real program leaves it out: the client then
    // sends with `concord::Http`, a `reqwest` client.
    let tasks = Tasks::new().with_transport(fake(|request| {
        println!("{} {}", request.method(), request.url());
        let body = request
            .body()
            .and_then(|body| body.as_bytes())
            .unwrap_or_default();
        println!("body: {}", String::from_utf8_lossy(body));
        Ok(reply(
            201,
            r#"{ "id": 7, "title": "Write the docs", "done": false }"#,
        ))
    }));

    // A required input is an argument. `.await` sends the call, and `?` passes the one
    // error type of this client, `tasks::Error`.
    let task = tasks
        .create(NewTask {
            title: "Write the docs".into(),
        })
        .await?;
    println!("created: {task:?}");
    println!(
        "task {} is {}",
        task.id,
        if task.done { "done" } else { "open" }
    );
    println!("title: {}", task.title);
    Ok(())
}

// It prints:
//
// POST https://tasks.example.com/api/tasks
// body: {"title":"Write the docs"}
// created: Task { id: 7, title: "Write the docs", done: false }
// task 7 is open
// title: Write the docs
