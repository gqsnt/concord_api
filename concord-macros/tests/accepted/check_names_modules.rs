//! An application's modules whose names the check could have used stay the application's.
pub mod __check_problem_1 {
    pub type Answer = String;
    pub fn reader() -> concord::Text {
        concord::Text
    }
}
pub mod __check_problem_2 {
    pub type Answer = String;
}
concord::api! {
    client Client {
        base "https://unused.invalid";
        GET a() -> __check_problem_1::Answer { read __check_problem_1::reader(); }
        GET b() -> __check_problem_2::Answer { read concord::Text; }
        GET c() -> String { read concord::Text; }
    }
}
fn main() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let client = Client::new().with_transport(concord::fake(|_| Ok(concord::reply(200, "yes"))));
            assert_eq!(client.a().await.unwrap(), "yes");
            assert_eq!(client.b().await.unwrap(), "yes");
            assert_eq!(client.c().await.unwrap(), "yes");
        });
}
