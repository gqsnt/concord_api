concord::api! {
    client Tasks {
        GET health();
        path "tasks/{task_id}";
    }
}
fn main() {}
