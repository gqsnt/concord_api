concord::api! { pub client Fixture {
base "https://fixture.example"; read Json;


scope contents(path: String) { path "contents/{*path}"; }


} }