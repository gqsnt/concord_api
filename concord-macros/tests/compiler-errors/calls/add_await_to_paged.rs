use concord::{by_offset, Json};
concord::api! {
    pub client Api {
        base "https://calls.test/";
        POST create();
        GET get();
        GET list(start?: u32) -> Vec<u32> {
            query { start; }
            read Json;
            paginate by_offset(start);
        }
    }
}
impl std::future::IntoFuture for api::Call<api::List> {
    type Output = ();
    type IntoFuture = std::future::Ready<()>;
    fn into_future(self) -> Self::IntoFuture {
        std::future::ready(())
    }
}
fn main() {}
