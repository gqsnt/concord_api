concord::api! {
pub client Tasks {
        base "https://tasks.example.com/api/tasks";
        read Json;

        GET get(id: u64) -> Task { path "{id}"; }
    }
}
