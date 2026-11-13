concord::api! { pub client Fixture {
base "https://fixture.example"; read Json;


GET list() -> Vec<Task>;


} }