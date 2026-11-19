#![allow(non_camel_case_types)]

type __Problem = u16;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    use api::*;
    concord::api! {
        client Api {
            base "https://wire.invalid";
            GET get() { query { "n" = std::mem::size_of::<__Problem>(); } }
        }
    }
    let api = Api::new().with_transport(concord::fake(|request| {
        assert_eq!(request.url().query(), Some("n=2"));
        Ok(concord::reply(204, ""))
    }));
    let call: Call<Get> = api.get();
    call.await.unwrap();
}
