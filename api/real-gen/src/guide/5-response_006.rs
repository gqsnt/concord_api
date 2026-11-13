concord::api! { pub client Fixture {
base "https://fixture.example"; read Json;


/// Is it there?
HEAD exists(id: DocId) -> bool {
    path "{id}";
    status { 2xx => true, 404 => false }
}


} }