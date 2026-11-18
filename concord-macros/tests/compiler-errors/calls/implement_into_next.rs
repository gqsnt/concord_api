#![allow(dead_code)]
// Only Concord's two pager results implement `IntoNext`, each for its own call.
concord::api! {
    pub client Api {
        base "https://calls.test/";
        GET get();
    }
}
struct LocalError;
type Generated = api::Call<api::Get>;
impl concord::page::IntoNext<Generated> for Result<Option<Generated>, LocalError> {
    fn into_next(self) -> Result<Option<Generated>, concord::Fail<()>> {
        self.map_err(|_| concord::mismatch("pager failure"))
    }
}
fn main() {}
