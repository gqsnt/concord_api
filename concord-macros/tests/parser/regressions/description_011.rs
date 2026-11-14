/// GitHub's uploads live on another host.
POST upload_asset(name: String, file: concord::Body) -> Asset {
    base "https://uploads.github.com";
    headers { override "authorization" = "Bearer {client.token}"; }     // or: remove "authorization";
    path "assets";
    query { name; }
    body file;
}
