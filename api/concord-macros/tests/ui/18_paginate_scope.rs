concord::api! {
    client Tasks {
        scope jobs {
            body pack_submission(job);
        }
    }
}
fn main() {}
