// macro.md 4 says ? in a reader expression does not compile. This does.
struct Local;
impl concord::read::Reader<Local> for Result<(), ()> {
    async fn read(&self, _: concord::read::Incoming) -> Result<Local, concord::read::ReadError> {
        Ok(Local)
    }
}
concord::api! {
    client Audit {
        base "https://audit.test";
        GET get() -> Local {
            read { let value: Result<Result<(), ()>, ()> = Ok(Ok(())); value? };
        }
    }
}
fn main() { let _ = Audit::new().get(); }
