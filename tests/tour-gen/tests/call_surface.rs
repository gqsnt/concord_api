//! The public caller contract exercised against the macro's clients.
extern crate tour_gen as tour;

#[path = "../../tour/tests/call_surface.rs"]
mod original;

concord::api! {
    client Rankings {
        base "https://rankings.example";
        scope champion_mastery {
            path "mastery";
            GET top(player: String, count?: u32) {
                path "{player}";
                query { count; }
            }
        }
    }
}

#[test]
fn c1_11_the_endpoint_function_and_fluent_setter_build_the_same_request() -> rankings::Result<()> {
    use rankings::champion_mastery::Top;

    let call = Rankings::new().champion_mastery().top("p-1").count(9);
    let associated = Top::count(call.clone(), 3).request()?;
    let fluent = call.count(3).request()?;
    assert_eq!(associated.method(), fluent.method());
    assert_eq!(associated.url(), fluent.url());
    assert_eq!(associated.headers(), fluent.headers());
    assert!(associated.body().is_none() && fluent.body().is_none());
    assert_eq!(
        associated.url().as_str(),
        "https://rankings.example/mastery/p-1?count=3"
    );
    Ok(())
}

#[test]
fn c8_8_files_single_response_calls_implement_into_future() {
    fn awaitable(_: impl std::future::IntoFuture) {}

    let files = tour::Files::new("test-key");
    awaitable(files.download(tour::FileId::from("file")));
    awaitable(files.exists(tour::DocId::from("doc")));
    awaitable(files.grant("client_credentials", "client", "secret"));
    awaitable(files.readme("repo"));
    awaitable(files.search("term"));
}
