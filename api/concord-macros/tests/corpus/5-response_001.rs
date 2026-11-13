GET readme(owner: String, repo: String) -> String {
    path "repos/{owner}/{repo}/readme";
    headers { override "accept" = "application/vnd.github.raw+json"; }
    read Text;
}
