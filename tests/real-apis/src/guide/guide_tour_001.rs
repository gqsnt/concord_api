concord::api! {
pub client Tasks {
        base "https://tasks.example.com/api/tasks";
        read Json;                                  // this API responds with JSON

        GET list(done?: bool) -> Vec<Task> {
            query { done; }
        }

        POST create(task: NewTask) -> Task {
            body json(task);
        }

        GET get(id: u64) -> Task { path "{id}"; }

        DELETE delete(id: u64) { path "{id}"; }
    }
}
