concord::api! { pub client Fixture {
base "https://fixture.example"; read Json;


/// Plain text, not JSON.
GET readme(repo: String) -> String {
    path "{repo}/readme";
    read Text;
}


} }