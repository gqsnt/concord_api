concord::api! { pub client Fixture {
base "https://fixture.example"; read Json;


POST send_photo(chat_id: ChatId, photo: concord::Body) -> Message {
    path "sendPhoto";
    body multipart()
        .text("chat_id", chat_id)
        .file("photo", "photo.jpg", "image/jpeg", photo)
        .finish();
}


} }