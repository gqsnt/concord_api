/// Plain text, not JSON.
GET readme(repo: String) -> String {
    path "{repo}/readme";
    read Text;
}
