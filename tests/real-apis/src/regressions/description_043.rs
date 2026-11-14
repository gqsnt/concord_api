concord::api! { pub client Fixture(key: Secret) {
base "https://fixture.example"; read Json;
headers { "authorization" = "Bearer {key}"; }

/// An upload, on another host.
PUT upload(name: String, file: concord::Body) {
    base "https://uploads.example.com";
    headers { override "authorization" = "Bearer {client.key}"; }    // the key goes to this host too
    path "files/{name}";
    body file;
}


} }