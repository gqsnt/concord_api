concord::api! { pub client Fixture {
base "https://fixture.example"; read Json;


/// A diff: a header of its own, and the bytes as they come.
GET diff() -> Bytes {
    headers { "accept" = "application/vnd.github.diff"; }
    read Binary;
}


} }