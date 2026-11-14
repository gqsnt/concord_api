concord::api! {
pub client Tasks {
        base "https://tasks.example.com/api/tasks";
        read Json;

        GET list(done?: bool) -> Vec<Task> { query { done; } }
        POST create(task: NewTask) -> Task { body json(task); }
        DELETE delete(id: u64) { path "{id}"; }
    }
}
