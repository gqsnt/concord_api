/// A large file, as it comes.
GET download(id: FileId) -> concord::ByteStream {
    path "files/{id}";
    read Stream;                       // `use concord::read::Stream;`
}
