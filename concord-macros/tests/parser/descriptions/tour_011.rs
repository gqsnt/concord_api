/// An upload, on another host.
PUT upload(name: String, file: concord::Body) {
    base "https://uploads.example.com";
    path "files/{name}";
    body file;
}
