concord::api! { pub client Fixture {
base "https://fixture.example"; read Json;


POST create(request: ChatRequest) -> ChatReply {
    body json(request);
    read ChatRead(request.stream);          // your reader: `Json` or `Stream`, by this flag
}


} }