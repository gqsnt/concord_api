concord::api! { pub client Fixture {
base "https://fixture.example"; read Json;


METHOD "PURGE" purge(key: String) { path "cache/{key}"; }


} }