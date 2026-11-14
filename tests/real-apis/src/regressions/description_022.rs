concord::api! { pub client Fixture {
base "https://fixture.example"; read Json;
headers { "accept" = "application/vnd.github+json"; }

GET readme(owner: String, repo: String) -> String {
    path "repos/{owner}/{repo}/readme";
    headers { override "accept" = "application/vnd.github.raw+json"; }
    read Text;
}


} }