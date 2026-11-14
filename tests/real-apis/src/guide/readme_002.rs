concord::api! {
client Tasks {
        base "https://tasks.example.com/api/tasks";
        read Json;

        POST create(task: NewTask) -> Task {
            body json(task);
        }
    }
}
