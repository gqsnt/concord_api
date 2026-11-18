//! A description whose `paginate` block holds an item under an attribute that expands to
//! `return |call, page| …;`. The macro cannot see that closure, so the expression takes the
//! inferred path, and the closure's parameters are inferred. It prints what the pages gave.
#![allow(dead_code, unreachable_code)]
use concord::{fake, reply_with, Json};

#[derive(serde::Deserialize)]
pub struct Page {
    pub items: Vec<u32>,
    pub next: Option<String>,
}

fn loop_forever() -> ! {
    loop {}
}

concord::api! {
    pub client Probe {
        base "https://probe.example.test";
        read Json;
        GET list(after?: String) -> Page {
            query { after; }
            paginate { #[concord_attr_probe::returns_a_closure] fn marker() {} loop_forever() };
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let transport = fake(|_| {
        Ok(reply_with(
            200,
            &[("content-type", "application/json")],
            r#"{"items":[1],"next":"n"}"#,
        ))
    });
    let mut pages = Probe::new().with_transport(transport).list().pages();
    let mut seen = Vec::new();
    loop {
        match pages.try_next().await {
            Ok(Some(page)) => seen.push(format!("page {:?}", page.items)),
            Ok(None) => break,
            Err(error) => seen.push(format!("{:?}", error.kind())),
        }
    }
    println!("{}", seen.join(", "));
}
