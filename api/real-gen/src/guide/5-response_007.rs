concord::api! { pub client Fixture {
base "https://fixture.example"; read Json;


POST create(task: NewTask) -> Created {
    body json(task);
    status { 2xx { body, headers } => created(body, headers)? }
}


} }